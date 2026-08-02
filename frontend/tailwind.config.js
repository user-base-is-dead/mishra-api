/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{vue,js,ts,jsx,tsx}'],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        // Primary — luminous Gold (Mishra API brand), hot glossy top end
        primary: {
          50:  '#fef9e6',
          100: '#fdefbf',
          200: '#fadd7d',
          300: '#ffcf3d',
          400: '#ffb61c',
          500: '#f0a50a',
          600: '#c68410',
          700: '#9d6410',
          800: '#7f4f14',
          900: '#6b4217',
          950: '#3e2409'
        },
        // Accent — warm neutral (replaces cold slate)
        accent: {
          50:  '#faf7f2',
          100: '#f0e9df',
          200: '#ded3c3',
          300: '#b8a890',
          400: '#8a7a63',
          500: '#5f5240',
          600: '#443a2c',
          700: '#2e2820',
          800: '#1d1611',
          900: '#130e08',
          950: '#0a0603'
        },
        // Dark mode backgrounds — deep lacquered espresso (glossy, matches gold)
        dark: {
          50:  '#faf7f2',
          100: '#f0e9df',
          200: '#ded3c3',
          300: '#b8a890',
          400: '#8a7a63',
          500: '#5f5240',
          600: '#443a2c',
          700: '#2e2820',
          800: '#1d1611',
          900: '#130e08',
          950: '#0a0603'
        }
      },
      fontFamily: {
        sans: [
          'Inter', 'system-ui', '-apple-system', 'BlinkMacSystemFont', 'Segoe UI',
          'Roboto', 'Helvetica Neue', 'Arial',
          'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', 'sans-serif'
        ],
        display: [
          'Space Grotesk', 'Inter', 'system-ui', '-apple-system', 'Segoe UI', 'sans-serif'
        ],
        mono: ['JetBrains Mono', 'ui-monospace', 'SFMono-Regular', 'Menlo', 'Monaco', 'Consolas', 'monospace']
      },
      boxShadow: {
        glass:       '0 8px 32px rgba(0, 0, 0, 0.08)',
        'glass-sm':  '0 4px 16px rgba(0, 0, 0, 0.06)',
        glow:        '0 0 20px rgba(212, 160, 23, 0.25)',
        'glow-lg':   '0 0 40px rgba(212, 160, 23, 0.35)',
        'gold-sm':   '0 2px 8px rgba(240, 165, 10, 0.22)',
        'gold-md':   '0 6px 24px rgba(240, 165, 10, 0.30)',
        'gold-lg':   '0 12px 48px rgba(198, 132, 16, 0.40)',
        'gold-glow': '0 0 24px rgba(255, 182, 28, 0.40), 0 0 72px rgba(240, 165, 10, 0.22)',
        'rim-light': 'inset 0 1px 0 rgba(255, 233, 163, 0.32)',
        'glass-deep':'0 42px 90px -28px rgba(0, 0, 0, 0.72), inset 0 1px 0 rgba(255, 233, 163, 0.10)',
        card:        '0 1px 3px rgba(0, 0, 0, 0.04), 0 1px 2px rgba(0, 0, 0, 0.06)',
        'card-hover':'0 10px 40px rgba(0, 0, 0, 0.08)',
        'inner-glow':'inset 0 1px 0 rgba(255, 255, 255, 0.1)'
      },
      backgroundImage: {
        'gradient-radial':      'radial-gradient(var(--tw-gradient-stops))',
        'gradient-primary':     'linear-gradient(135deg, #e0a82e 0%, #b8860b 100%)',
        'gradient-gold':        'linear-gradient(135deg, #f4e0a0 0%, #d4a017 45%, #b8860b 100%)',
        'gradient-gold-sheen':  'linear-gradient(135deg, #ecc968 0%, #d4a017 50%, #96690c 100%)',
        'gradient-gold-text':   'linear-gradient(90deg, #b8860b 0%, #d4a017 50%, #ecc968 100%)',
        'gradient-gold-hot':    'linear-gradient(135deg, #ffe9a3 0%, #ffb61c 42%, #c77b06 100%)',
        'gradient-sheen':       'linear-gradient(110deg, transparent 25%, rgba(255, 233, 163, 0.55) 50%, transparent 75%)',
        'gradient-dark':        'linear-gradient(135deg, #1d1611 0%, #0a0603 100%)',
        'gradient-glass':       'linear-gradient(135deg, rgba(255,255,255,0.1) 0%, rgba(255,255,255,0.05) 100%)',
        'mesh-gradient':
          'radial-gradient(at 40% 20%, rgba(212,160,23,0.12) 0px, transparent 50%), radial-gradient(at 80% 0%, rgba(184,134,11,0.08) 0px, transparent 50%), radial-gradient(at 0% 50%, rgba(224,168,46,0.08) 0px, transparent 50%)'
      },
      animation: {
        'fade-in':       'fadeIn 0.3s ease-out',
        'slide-up':      'slideUp 0.3s ease-out',
        'slide-down':    'slideDown 0.3s ease-out',
        'slide-in-right':'slideInRight 0.3s ease-out',
        'scale-in':      'scaleIn 0.2s ease-out',
        'pulse-slow':    'pulse 3s cubic-bezier(0.4, 0, 0.6, 1) infinite',
        shimmer:         'shimmer 2s linear infinite',
        glow:            'glow 2s ease-in-out infinite alternate'
      },
      keyframes: {
        fadeIn:       { '0%': { opacity: '0' }, '100%': { opacity: '1' } },
        slideUp:      { '0%': { opacity: '0', transform: 'translateY(10px)' },  '100%': { opacity: '1', transform: 'translateY(0)' } },
        slideDown:    { '0%': { opacity: '0', transform: 'translateY(-10px)' }, '100%': { opacity: '1', transform: 'translateY(0)' } },
        slideInRight: { '0%': { opacity: '0', transform: 'translateX(20px)' },  '100%': { opacity: '1', transform: 'translateX(0)' } },
        scaleIn:      { '0%': { opacity: '0', transform: 'scale(0.95)' },       '100%': { opacity: '1', transform: 'scale(1)' } },
        shimmer:      { '0%': { backgroundPosition: '-200% 0' }, '100%': { backgroundPosition: '200% 0' } },
        glow:         { '0%': { boxShadow: '0 0 20px rgba(212,160,23,0.25)' }, '100%': { boxShadow: '0 0 30px rgba(212,160,23,0.4)' } }
      },
      backdropBlur: { xs: '2px' },
      borderRadius: { '4xl': '2rem' }
    }
  },
  plugins: []
}
