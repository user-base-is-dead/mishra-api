/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Miron Code contributors. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

/**
 * A GPU fluid solver, used to render drifting smoke behind the workbench.
 *
 * This is Stam's stable-fluids scheme, the standard real-time approximation of
 * incompressible Navier-Stokes. Each step runs as a chain of full-screen passes
 * over ping-ponged float textures:
 *
 *   1. advect velocity along itself      (semi-Lagrangian, unconditionally stable)
 *   2. compute curl, then apply vorticity confinement
 *   3. compute divergence of the velocity field
 *   4. solve for pressure with Jacobi iterations
 *   5. subtract the pressure gradient, leaving a divergence-free field
 *   6. advect the dye along that field
 *
 * Step 5 is what makes it look like smoke rather than like noise: a
 * divergence-free field conserves volume, so the dye folds and curls instead of
 * simply fading in and out. Vorticity confinement in step 2 pushes energy back
 * into the small eddies that the coarse grid would otherwise smear away, which
 * is where the wisps come from.
 *
 * The dye is a single scalar density. Colour is applied only at display time, by
 * ramping that density through the theme's palette, so the smoke is always the
 * product's own gold rather than something baked into the simulation.
 */

export interface IFluidColor {
	readonly r: number;
	readonly g: number;
	readonly b: number;
}

// --- Shader sources ---------------------------------------------------------

/**
 * Every pass is a full-screen quad, and every pass but advection needs its four
 * neighbouring texels, so the base vertex shader computes them once here rather
 * than in each fragment shader.
 */
const VERTEX_SHADER = `
precision highp float;

attribute vec2 aPosition;
uniform vec2 uTexelSize;

varying vec2 vUv;
varying vec2 vL;
varying vec2 vR;
varying vec2 vT;
varying vec2 vB;

void main() {
	vUv = aPosition * 0.5 + 0.5;
	vL = vUv - vec2(uTexelSize.x, 0.0);
	vR = vUv + vec2(uTexelSize.x, 0.0);
	vT = vUv + vec2(0.0, uTexelSize.y);
	vB = vUv - vec2(0.0, uTexelSize.y);
	gl_Position = vec4(aPosition, 0.0, 1.0);
}
`;

const FRAGMENT_PRELUDE = `
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
precision highp sampler2D;

varying vec2 vUv;
varying vec2 vL;
varying vec2 vR;
varying vec2 vT;
varying vec2 vB;
`;

/**
 * Semi-Lagrangian advection: trace backwards along the velocity field and read
 * what was there. Stable at any timestep, which is what lets the solver run at
 * a fixed step regardless of frame rate.
 */
const ADVECTION_SHADER = FRAGMENT_PRELUDE + `
uniform sampler2D uVelocity;
uniform sampler2D uSource;
uniform vec2 uTexelSize;
uniform float uDt;
uniform float uDissipation;

void main() {
	vec2 coord = vUv - uDt * texture2D(uVelocity, vUv).xy * uTexelSize;
	gl_FragColor = uDissipation * texture2D(uSource, coord);
	gl_FragColor.a = 1.0;
}
`;

const DIVERGENCE_SHADER = FRAGMENT_PRELUDE + `
uniform sampler2D uVelocity;

void main() {
	float l = texture2D(uVelocity, vL).x;
	float r = texture2D(uVelocity, vR).x;
	float t = texture2D(uVelocity, vT).y;
	float b = texture2D(uVelocity, vB).y;

	// Reflect the field at the boundary so smoke does not leak off the edges.
	vec2 c = texture2D(uVelocity, vUv).xy;
	if (vL.x < 0.0) { l = -c.x; }
	if (vR.x > 1.0) { r = -c.x; }
	if (vT.y > 1.0) { t = -c.y; }
	if (vB.y < 0.0) { b = -c.y; }

	gl_FragColor = vec4(0.5 * (r - l + t - b), 0.0, 0.0, 1.0);
}
`;

const CURL_SHADER = FRAGMENT_PRELUDE + `
uniform sampler2D uVelocity;

void main() {
	float l = texture2D(uVelocity, vL).y;
	float r = texture2D(uVelocity, vR).y;
	float t = texture2D(uVelocity, vT).x;
	float b = texture2D(uVelocity, vB).x;
	gl_FragColor = vec4(0.5 * (r - l - t + b), 0.0, 0.0, 1.0);
}
`;

/**
 * Vorticity confinement. A grid this coarse dissipates small eddies within a few
 * steps; this measures where curl is concentrated and pushes velocity back
 * towards it, restoring the fine curls that make smoke look like smoke.
 */
const VORTICITY_SHADER = FRAGMENT_PRELUDE + `
uniform sampler2D uVelocity;
uniform sampler2D uCurl;
uniform float uCurlStrength;
uniform float uDt;

void main() {
	float l = texture2D(uCurl, vL).x;
	float r = texture2D(uCurl, vR).x;
	float t = texture2D(uCurl, vT).x;
	float b = texture2D(uCurl, vB).x;
	float c = texture2D(uCurl, vUv).x;

	vec2 force = 0.5 * vec2(abs(t) - abs(b), abs(r) - abs(l));
	force /= length(force) + 0.0001;
	force *= uCurlStrength * c;
	force.y *= -1.0;

	vec2 velocity = texture2D(uVelocity, vUv).xy;
	velocity += force * uDt;
	velocity = clamp(velocity, -1000.0, 1000.0);
	gl_FragColor = vec4(velocity, 0.0, 1.0);
}
`;

/** One Jacobi iteration of the pressure Poisson equation. */
const PRESSURE_SHADER = FRAGMENT_PRELUDE + `
uniform sampler2D uPressure;
uniform sampler2D uDivergence;

void main() {
	float l = texture2D(uPressure, vL).x;
	float r = texture2D(uPressure, vR).x;
	float t = texture2D(uPressure, vT).x;
	float b = texture2D(uPressure, vB).x;
	float divergence = texture2D(uDivergence, vUv).x;
	gl_FragColor = vec4((l + r + b + t - divergence) * 0.25, 0.0, 0.0, 1.0);
}
`;

/** Projects the velocity field back to divergence-free. */
const GRADIENT_SUBTRACT_SHADER = FRAGMENT_PRELUDE + `
uniform sampler2D uPressure;
uniform sampler2D uVelocity;

void main() {
	float l = texture2D(uPressure, vL).x;
	float r = texture2D(uPressure, vR).x;
	float t = texture2D(uPressure, vT).x;
	float b = texture2D(uPressure, vB).x;
	vec2 velocity = texture2D(uVelocity, vUv).xy;
	velocity -= vec2(r - l, t - b);
	gl_FragColor = vec4(velocity, 0.0, 1.0);
}
`;

const CLEAR_SHADER = FRAGMENT_PRELUDE + `
uniform sampler2D uTexture;
uniform float uValue;

void main() {
	gl_FragColor = uValue * texture2D(uTexture, vUv);
}
`;

/** Adds a gaussian blob of velocity or dye. */
const SPLAT_SHADER = FRAGMENT_PRELUDE + `
uniform sampler2D uTarget;
uniform float uAspect;
uniform vec3 uValue;
uniform vec2 uPoint;
uniform float uRadius;

void main() {
	vec2 p = vUv - uPoint;
	p.x *= uAspect;
	vec3 splat = exp(-dot(p, p) / uRadius) * uValue;
	vec3 base = texture2D(uTarget, vUv).xyz;
	gl_FragColor = vec4(base + splat, 1.0);
}
`;

/**
 * Maps dye density onto the theme palette. The simulation itself is colourless;
 * everything the user sees as "gold" happens here.
 */
const DISPLAY_SHADER = FRAGMENT_PRELUDE + `
uniform sampler2D uDye;
uniform vec3 uBase;
uniform vec3 uMid;
uniform vec3 uHigh;
uniform float uIntensity;
uniform float uAspect;

void main() {
	float d = texture2D(uDye, vUv).x;

	// Two overlapping ramps: density first washes the surface towards the band
	// colour, then the densest cores pick up the highlight. A single ramp gives a
	// flat wash with no sense of depth.
	vec3 col = mix(uBase, uMid, clamp(d * 1.15, 0.0, 1.0));
	col = mix(col, uHigh, clamp((d - 0.55) * 1.4, 0.0, 1.0));
	col = mix(uBase, col, uIntensity);

	// A gentle vignette. The window is far wider than it is tall, so the falloff
	// is measured on a squared-up coordinate to avoid pinching the sides.
	vec2 c = (vUv - 0.5) * vec2(uAspect, 1.0);
	float vig = smoothstep(1.5, 0.35, length(c));
	col = mix(uBase, col, mix(0.85, 1.0, vig));

	gl_FragColor = vec4(col, 1.0);
}
`;

// --- WebGL plumbing ---------------------------------------------------------

interface ITextureFormat {
	readonly internalFormat: number;
	readonly format: number;
}

interface IGLSupport {
	readonly halfFloatType: number;
	readonly filtering: number;
	readonly rgba: ITextureFormat;
	readonly rg: ITextureFormat;
	readonly r: ITextureFormat;
}

interface IFramebuffer {
	readonly texture: WebGLTexture;
	readonly fbo: WebGLFramebuffer;
	readonly width: number;
	readonly height: number;
	readonly texelSizeX: number;
	readonly texelSizeY: number;
	attach(unit: number): number;
}

interface IDoubleFramebuffer {
	read: IFramebuffer;
	write: IFramebuffer;
	swap(): void;
}

function compile(gl: WebGLRenderingContext, type: number, source: string): WebGLShader | undefined {
	const shader = gl.createShader(type);
	if (!shader) {
		return undefined;
	}

	gl.shaderSource(shader, source);
	gl.compileShader(shader);

	if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
		gl.deleteShader(shader);
		return undefined;
	}

	return shader;
}

/** A linked program with its uniform locations resolved once up front. */
class Pass {

	private readonly uniforms = new Map<string, WebGLUniformLocation | null>();

	private constructor(
		private readonly gl: WebGLRenderingContext,
		readonly program: WebGLProgram
	) {
		const count = gl.getProgramParameter(program, gl.ACTIVE_UNIFORMS) as number;
		for (let i = 0; i < count; i++) {
			const info = gl.getActiveUniform(program, i);
			if (info) {
				this.uniforms.set(info.name, gl.getUniformLocation(program, info.name));
			}
		}
	}

	static create(gl: WebGLRenderingContext, vertex: WebGLShader, fragmentSource: string): Pass | undefined {
		const fragment = compile(gl, gl.FRAGMENT_SHADER, fragmentSource);
		if (!fragment) {
			return undefined;
		}

		const program = gl.createProgram();
		if (!program) {
			gl.deleteShader(fragment);
			return undefined;
		}
		gl.attachShader(program, vertex);
		gl.attachShader(program, fragment);
		// The quad buffer is bound to location 0 once, for every pass.
		gl.bindAttribLocation(program, 0, 'aPosition');
		gl.linkProgram(program);
		gl.deleteShader(fragment);

		if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
			gl.deleteProgram(program);
			return undefined;
		}

		return new Pass(gl, program);
	}

	use(): void {
		this.gl.useProgram(this.program);
	}

	uniform(name: string): WebGLUniformLocation | null {
		return this.uniforms.get(name) ?? null;
	}

	dispose(): void {
		this.gl.deleteProgram(this.program);
	}
}

// --- The simulation ---------------------------------------------------------

export interface IFluidPalette {
	readonly base: IFluidColor;
	readonly mid: IFluidColor;
	readonly high: IFluidColor;
}

export interface IFluidOptions {
	/** 0 flattens the field to the base tone; 1 is full strength. */
	intensity: number;
	/** Whether the pointer disturbs the smoke. */
	followPointer: boolean;
}

/**
 * Grid resolutions. Velocity is solved coarse — the pressure solve is the
 * expensive part and low frequencies are all that matter for the motion — while
 * the dye is advected finer, because that is what is actually visible.
 */
const SIM_RESOLUTION = 128;
const DYE_RESOLUTION = 512;

/** Jacobi iterations per step. Twenty is the usual quality/cost knee. */
const PRESSURE_ITERATIONS = 20;

/**
 * Tuning. These are the difference between "smoke" and "a lava lamp":
 *
 * `VELOCITY_DISSIPATION` damps motion so the field settles rather than sloshing
 * forever. `CURL` is vorticity confinement — too little and the smoke is
 * featureless, too much and it boils.
 *
 * Dye dissipation is the parameter that decides whether this looks like smoke at
 * all. Too slow and a steady stream of injections mixes to a flat, uniform wash
 * — every pixel ends at the same density and there is no structure left to see.
 * Smoke is *contrast*: dense wisps against clear space, which needs a half-life
 * short enough that each injection fades before it has finished blending into
 * the last.
 *
 * Both are expressed as the fraction surviving after one *second*, not one
 * step. Per-step values would silently change the look whenever the frame rate
 * changed; this way the frame rate is purely a cost dial.
 */
const VELOCITY_DISSIPATION_PER_SECOND = 0.547;
const DENSITY_DISSIPATION_PER_SECOND = 0.617;
const CURL = 22;
const SPLAT_RADIUS = 0.01;

export class FluidSimulation {

	private readonly gl: WebGLRenderingContext;
	private readonly support: IGLSupport;

	private readonly vertexShader: WebGLShader;
	private readonly passes: {
		advection: Pass;
		divergence: Pass;
		curl: Pass;
		vorticity: Pass;
		pressure: Pass;
		gradient: Pass;
		clear: Pass;
		splat: Pass;
		display: Pass;
	};

	private readonly quad: WebGLBuffer;
	private readonly indices: WebGLBuffer;

	private velocity!: IDoubleFramebuffer;
	private dye!: IDoubleFramebuffer;
	private pressure!: IDoubleFramebuffer;
	private divergence!: IFramebuffer;
	private curl!: IFramebuffer;

	private disposed = false;

	private constructor(
		private readonly canvas: HTMLCanvasElement,
		gl: WebGLRenderingContext,
		support: IGLSupport,
		vertexShader: WebGLShader,
		passes: FluidSimulation['passes'],
		private palette: IFluidPalette,
		private options: IFluidOptions
	) {
		this.gl = gl;
		this.support = support;
		this.vertexShader = vertexShader;
		this.passes = passes;

		const quad = gl.createBuffer();
		const indices = gl.createBuffer();
		if (!quad || !indices) {
			if (quad) { gl.deleteBuffer(quad); }
			if (indices) { gl.deleteBuffer(indices); }
			throw new Error('Unable to allocate fluid simulation buffers.');
		}

		this.quad = quad;
		gl.bindBuffer(gl.ARRAY_BUFFER, this.quad);
		gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, -1, 1, 1, 1, 1, -1]), gl.STATIC_DRAW);

		this.indices = indices;
		gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, this.indices);
		gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, new Uint16Array([0, 1, 2, 0, 2, 3]), gl.STATIC_DRAW);

		gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
		gl.enableVertexAttribArray(0);

		this.createFramebuffers();
	}

	/**
	 * Returns undefined when the driver cannot render to float textures, which is
	 * the one hard requirement — every field in the solver is a float texture.
	 */
	static create(
		canvas: HTMLCanvasElement,
		palette: IFluidPalette,
		options: IFluidOptions
	): FluidSimulation | undefined {
		const attributes: WebGLContextAttributes = {
			alpha: false,
			antialias: false,
			depth: false,
			stencil: false,
			powerPreference: 'high-performance',
			preserveDrawingBuffer: false
		};

		const gl = (canvas.getContext('webgl2', attributes)
			?? canvas.getContext('webgl', attributes)) as WebGLRenderingContext | null;

		if (!gl) {
			return undefined;
		}

		const support = FluidSimulation.probeSupport(gl);
		if (!support) {
			return undefined;
		}

		const vertexShader = compile(gl, gl.VERTEX_SHADER, VERTEX_SHADER);
		if (!vertexShader) {
			return undefined;
		}

		const sources = {
			advection: ADVECTION_SHADER,
			divergence: DIVERGENCE_SHADER,
			curl: CURL_SHADER,
			vorticity: VORTICITY_SHADER,
			pressure: PRESSURE_SHADER,
			gradient: GRADIENT_SUBTRACT_SHADER,
			clear: CLEAR_SHADER,
			splat: SPLAT_SHADER,
			display: DISPLAY_SHADER
		};

		const passes: Partial<FluidSimulation['passes']> = {};
		for (const [name, source] of Object.entries(sources)) {
			const pass = Pass.create(gl, vertexShader, source);
			if (!pass) {
				return undefined;
			}
			passes[name as keyof FluidSimulation['passes']] = pass;
		}

		return new FluidSimulation(
			canvas, gl, support, vertexShader,
			passes as FluidSimulation['passes'], palette, options
		);
	}

	private static probeSupport(gl: WebGLRenderingContext): IGLSupport | undefined {
		const isWebGL2 = typeof WebGL2RenderingContext !== 'undefined' && gl instanceof WebGL2RenderingContext;

		let halfFloatType: number;
		let linearSupported: boolean;

		if (isWebGL2) {
			const gl2 = gl as unknown as WebGL2RenderingContext;
			gl.getExtension('EXT_color_buffer_float');
			linearSupported = Boolean(gl.getExtension('OES_texture_float_linear'));
			halfFloatType = gl2.HALF_FLOAT;
		} else {
			const halfFloat = gl.getExtension('OES_texture_half_float');
			if (!halfFloat) {
				return undefined;
			}
			linearSupported = Boolean(gl.getExtension('OES_texture_half_float_linear'));
			halfFloatType = (halfFloat as { HALF_FLOAT_OES: number }).HALF_FLOAT_OES;
		}

		const gl2 = gl as unknown as WebGL2RenderingContext;
		const rgba: ITextureFormat = isWebGL2
			? { internalFormat: gl2.RGBA16F, format: gl.RGBA }
			: { internalFormat: gl.RGBA, format: gl.RGBA };
		const rg: ITextureFormat = isWebGL2
			? { internalFormat: gl2.RG16F, format: gl2.RG }
			: { internalFormat: gl.RGBA, format: gl.RGBA };
		const r: ITextureFormat = isWebGL2
			? { internalFormat: gl2.R16F, format: gl2.RED }
			: { internalFormat: gl.RGBA, format: gl.RGBA };

		// Confirm the driver will actually render into one, rather than only
		// claiming the extension.
		if (!FluidSimulation.canRenderTo(gl, rgba, halfFloatType)) {
			return undefined;
		}

		return {
			halfFloatType,
			filtering: linearSupported ? gl.LINEAR : gl.NEAREST,
			rgba,
			rg,
			r
		};
	}

	private static canRenderTo(gl: WebGLRenderingContext, format: ITextureFormat, type: number): boolean {
		const texture = gl.createTexture();
		gl.bindTexture(gl.TEXTURE_2D, texture);
		gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
		gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
		gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
		gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
		gl.texImage2D(gl.TEXTURE_2D, 0, format.internalFormat, 4, 4, 0, format.format, type, null);

		const fbo = gl.createFramebuffer();
		gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
		gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
		const complete = gl.checkFramebufferStatus(gl.FRAMEBUFFER) === gl.FRAMEBUFFER_COMPLETE;

		gl.bindFramebuffer(gl.FRAMEBUFFER, null);
		gl.deleteFramebuffer(fbo);
		gl.deleteTexture(texture);

		return complete;
	}

	// --- Framebuffers -------------------------------------------------------

	private createFramebuffer(width: number, height: number, format: ITextureFormat, filter: number): IFramebuffer {
		const gl = this.gl;

		gl.activeTexture(gl.TEXTURE0);
		const texture = gl.createTexture();
		if (!texture) {
			throw new Error('Unable to allocate a fluid simulation texture.');
		}
		gl.bindTexture(gl.TEXTURE_2D, texture);
		gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, filter);
		gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, filter);
		gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
		gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
		gl.texImage2D(gl.TEXTURE_2D, 0, format.internalFormat, width, height, 0, format.format, this.support.halfFloatType, null);

		const fbo = gl.createFramebuffer();
		if (!fbo) {
			gl.deleteTexture(texture);
			throw new Error('Unable to allocate a fluid simulation framebuffer.');
		}
		gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
		gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, texture, 0);
		gl.viewport(0, 0, width, height);
		gl.clear(gl.COLOR_BUFFER_BIT);

		return {
			texture,
			fbo,
			width,
			height,
			texelSizeX: 1 / width,
			texelSizeY: 1 / height,
			attach(unit: number): number {
				gl.activeTexture(gl.TEXTURE0 + unit);
				gl.bindTexture(gl.TEXTURE_2D, texture);
				return unit;
			}
		};
	}

	private createDoubleFramebuffer(width: number, height: number, format: ITextureFormat, filter: number): IDoubleFramebuffer {
		let read = this.createFramebuffer(width, height, format, filter);
		let write = this.createFramebuffer(width, height, format, filter);

		return {
			get read() { return read; },
			set read(value: IFramebuffer) { read = value; },
			get write() { return write; },
			set write(value: IFramebuffer) { write = value; },
			swap() {
				const temp = read;
				read = write;
				write = temp;
			}
		};
	}

	/** Grid dimensions for a target resolution, matched to the canvas aspect. */
	private resolution(target: number): { width: number; height: number } {
		const aspect = Math.max(this.canvas.width, 1) / Math.max(this.canvas.height, 1);
		const min = Math.round(target);
		const max = Math.round(target * (aspect > 1 ? aspect : 1 / aspect));

		return aspect > 1 ? { width: max, height: min } : { width: min, height: max };
	}

	private createFramebuffers(): void {
		const sim = this.resolution(SIM_RESOLUTION);
		const dye = this.resolution(DYE_RESOLUTION);
		const filter = this.support.filtering;

		this.velocity = this.createDoubleFramebuffer(sim.width, sim.height, this.support.rg, filter);
		this.dye = this.createDoubleFramebuffer(dye.width, dye.height, this.support.r, filter);
		this.pressure = this.createDoubleFramebuffer(sim.width, sim.height, this.support.r, this.gl.NEAREST);
		this.divergence = this.createFramebuffer(sim.width, sim.height, this.support.r, this.gl.NEAREST);
		this.curl = this.createFramebuffer(sim.width, sim.height, this.support.r, this.gl.NEAREST);
	}

	private destroyFramebuffers(): void {
		const gl = this.gl;
		const all = [
			this.velocity?.read, this.velocity?.write,
			this.dye?.read, this.dye?.write,
			this.pressure?.read, this.pressure?.write,
			this.divergence, this.curl
		];

		for (const target of all) {
			if (target) {
				gl.deleteTexture(target.texture);
				gl.deleteFramebuffer(target.fbo);
			}
		}
	}

	private blit(target: IFramebuffer | null): void {
		const gl = this.gl;

		if (target) {
			gl.viewport(0, 0, target.width, target.height);
			gl.bindFramebuffer(gl.FRAMEBUFFER, target.fbo);
		} else {
			gl.viewport(0, 0, this.canvas.width, this.canvas.height);
			gl.bindFramebuffer(gl.FRAMEBUFFER, null);
		}

		gl.drawElements(gl.TRIANGLES, 6, gl.UNSIGNED_SHORT, 0);
	}

	/** Sets uTexelSize for the grid a pass is about to read from. */
	private setTexelSize(pass: Pass, target: IFramebuffer): void {
		this.gl.uniform2f(pass.uniform('uTexelSize'), target.texelSizeX, target.texelSizeY);
	}

	// --- Public API ---------------------------------------------------------

	resize(width: number, height: number): void {
		if (this.disposed || (this.canvas.width === width && this.canvas.height === height)) {
			return;
		}

		this.canvas.width = width;
		this.canvas.height = height;

		// The grids are sized from the canvas aspect, so they have to be rebuilt.
		// The smoke restarts, which is invisible next to a window resize.
		this.destroyFramebuffers();
		this.createFramebuffers();
	}

	updatePalette(palette: IFluidPalette): void {
		this.palette = palette;
	}

	updateOptions(options: IFluidOptions): void {
		this.options = options;
	}

	/**
	 * Injects smoke and a push at a point. Coordinates are in [0, 1] with the
	 * origin at the bottom left, matching texture space.
	 */
	splat(x: number, y: number, dx: number, dy: number, density: number): void {
		if (this.disposed) {
			return;
		}

		const gl = this.gl;
		const aspect = Math.max(this.canvas.width, 1) / Math.max(this.canvas.height, 1);
		const pass = this.passes.splat;

		pass.use();
		this.setTexelSize(pass, this.velocity.read);
		gl.uniform1i(pass.uniform('uTarget'), this.velocity.read.attach(0));
		gl.uniform1f(pass.uniform('uAspect'), aspect);
		gl.uniform2f(pass.uniform('uPoint'), x, y);
		gl.uniform3f(pass.uniform('uValue'), dx, dy, 0);
		gl.uniform1f(pass.uniform('uRadius'), SPLAT_RADIUS);
		this.blit(this.velocity.write);
		this.velocity.swap();

		this.setTexelSize(pass, this.dye.read);
		gl.uniform1i(pass.uniform('uTarget'), this.dye.read.attach(0));
		gl.uniform3f(pass.uniform('uValue'), density, density, density);
		this.blit(this.dye.write);
		this.dye.swap();
	}

	/**
	 * Advances the solver by `dt` seconds.
	 *
	 * The caller owns the frame rate, so it owns the timestep. Semi-Lagrangian
	 * advection is unconditionally stable, so any reasonable value works and the
	 * smoke moves at the same wall-clock speed however often this is called.
	 */
	step(dt: number): void {
		if (this.disposed) {
			return;
		}

		const gl = this.gl;
		const velocityDissipation = Math.pow(VELOCITY_DISSIPATION_PER_SECOND, dt);
		const densityDissipation = Math.pow(DENSITY_DISSIPATION_PER_SECOND, dt);
		const { advection, divergence, curl, vorticity, pressure, gradient, clear } = this.passes;

		gl.disable(gl.BLEND);

		// Curl, then vorticity confinement.
		curl.use();
		this.setTexelSize(curl, this.velocity.read);
		gl.uniform1i(curl.uniform('uVelocity'), this.velocity.read.attach(0));
		this.blit(this.curl);

		vorticity.use();
		this.setTexelSize(vorticity, this.velocity.read);
		gl.uniform1i(vorticity.uniform('uVelocity'), this.velocity.read.attach(0));
		gl.uniform1i(vorticity.uniform('uCurl'), this.curl.attach(1));
		gl.uniform1f(vorticity.uniform('uCurlStrength'), CURL);
		gl.uniform1f(vorticity.uniform('uDt'), dt);
		this.blit(this.velocity.write);
		this.velocity.swap();

		// Divergence of the current field.
		divergence.use();
		this.setTexelSize(divergence, this.velocity.read);
		gl.uniform1i(divergence.uniform('uVelocity'), this.velocity.read.attach(0));
		this.blit(this.divergence);

		// Decay the previous pressure rather than starting from zero: the field
		// changes little between frames, so this converges in far fewer iterations.
		clear.use();
		this.setTexelSize(clear, this.pressure.read);
		gl.uniform1i(clear.uniform('uTexture'), this.pressure.read.attach(0));
		gl.uniform1f(clear.uniform('uValue'), 0.8);
		this.blit(this.pressure.write);
		this.pressure.swap();

		pressure.use();
		this.setTexelSize(pressure, this.pressure.read);
		gl.uniform1i(pressure.uniform('uDivergence'), this.divergence.attach(0));
		for (let i = 0; i < PRESSURE_ITERATIONS; i++) {
			gl.uniform1i(pressure.uniform('uPressure'), this.pressure.read.attach(1));
			this.blit(this.pressure.write);
			this.pressure.swap();
		}

		gradient.use();
		this.setTexelSize(gradient, this.velocity.read);
		gl.uniform1i(gradient.uniform('uPressure'), this.pressure.read.attach(0));
		gl.uniform1i(gradient.uniform('uVelocity'), this.velocity.read.attach(1));
		this.blit(this.velocity.write);
		this.velocity.swap();

		// Advect the velocity along itself, then the dye along the velocity.
		advection.use();
		this.setTexelSize(advection, this.velocity.read);
		gl.uniform1f(advection.uniform('uDt'), dt);
		gl.uniform1i(advection.uniform('uVelocity'), this.velocity.read.attach(0));
		gl.uniform1i(advection.uniform('uSource'), this.velocity.read.attach(0));
		gl.uniform1f(advection.uniform('uDissipation'), velocityDissipation);
		this.blit(this.velocity.write);
		this.velocity.swap();

		// The backtrace converts velocity into a texture-space offset, and velocity
		// is measured in cells of the *simulation* grid. So this stays on the
		// velocity texel size even though the dye grid is four times finer —
		// using the dye's would shrink every backtrace by that same factor, and
		// the smoke would crawl while piling up wherever it was injected.
		this.setTexelSize(advection, this.velocity.read);
		gl.uniform1i(advection.uniform('uVelocity'), this.velocity.read.attach(0));
		gl.uniform1i(advection.uniform('uSource'), this.dye.read.attach(1));
		gl.uniform1f(advection.uniform('uDissipation'), densityDissipation);
		this.blit(this.dye.write);
		this.dye.swap();
	}

	/** Draws the dye through the palette to the canvas. */
	render(): void {
		if (this.disposed) {
			return;
		}

		const gl = this.gl;
		const pass = this.passes.display;
		const toVec3 = (color: IFluidColor): [number, number, number] => {
			const { r, g, b } = color;
			return [r / 255, g / 255, b / 255];
		};

		const base = toVec3(this.palette.base);
		const mid = toVec3(this.palette.mid);
		const high = toVec3(this.palette.high);

		pass.use();
		this.setTexelSize(pass, this.dye.read);
		gl.uniform1i(pass.uniform('uDye'), this.dye.read.attach(0));
		gl.uniform3f(pass.uniform('uBase'), base[0], base[1], base[2]);
		gl.uniform3f(pass.uniform('uMid'), mid[0], mid[1], mid[2]);
		gl.uniform3f(pass.uniform('uHigh'), high[0], high[1], high[2]);
		gl.uniform1f(pass.uniform('uIntensity'), this.options.intensity);
		gl.uniform1f(pass.uniform('uAspect'), Math.max(this.canvas.width, 1) / Math.max(this.canvas.height, 1));
		this.blit(null);
	}

	dispose(): void {
		if (this.disposed) {
			return;
		}
		this.disposed = true;

		const gl = this.gl;
		this.destroyFramebuffers();

		for (const pass of Object.values(this.passes)) {
			pass.dispose();
		}
		gl.deleteShader(this.vertexShader);
		gl.deleteBuffer(this.quad);
		gl.deleteBuffer(this.indices);

		// Release the drawing buffer now rather than waiting for collection.
		gl.getExtension('WEBGL_lose_context')?.loseContext();
	}
}
