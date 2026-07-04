import type { ReactNode } from 'react'

/**
 * lightweight markdown renderer(dedicatedas GitHub Release Notes scenario).
 *
 * override:title(#–####),paragraph,hasorder/unorderedList,`> reference`,`---` divider,
 * fenced code block(``` fenced ```),inline `code`,`**bold**`,`*italic*`,`[text](url)`.
 *
 * notSupported:nestedList,table,HTML,image,footnote.
 * Inputfrom a trusted source(GitHub Release body),React text nodes are escaped automatically.
 */

interface MarkdownProps {
  text: string
  className?: string
}

export function Markdown({ text, className }: MarkdownProps) {
  const blocks = parseBlocks(text || '')
  return (
    <div className={`md-prose space-y-2 ${className ?? ''}`}>
      {blocks.map((block, i) => renderBlock(block, i))}
    </div>
  )
}

type Block =
  | { kind: 'heading'; level: 1 | 2 | 3 | 4; text: string }
  | { kind: 'paragraph'; text: string }
  | { kind: 'ulist'; items: string[] }
  | { kind: 'olist'; items: string[] }
  | { kind: 'quote'; text: string }
  | { kind: 'code'; lang?: string; content: string }
  | { kind: 'hr' }

function parseBlocks(src: string): Block[] {
  const lines = src.replace(/\r\n?/g, '\n').split('\n')
  const blocks: Block[] = []
  let i = 0

  while (i < lines.length) {
    const line = lines[i]

    const fence = line.match(/^```(\w+)?\s*$/)
    if (fence) {
      const lang = fence[1]
      const buf: string[] = []
      i++
      while (i < lines.length && !/^```\s*$/.test(lines[i])) {
        buf.push(lines[i])
        i++
      }
      if (i < lines.length) i++
      blocks.push({ kind: 'code', lang, content: buf.join('\n') })
      continue
    }

    if (/^\s*$/.test(line)) {
      i++
      continue
    }

    if (/^\s*(?:---|\*\*\*|___)\s*$/.test(line)) {
      blocks.push({ kind: 'hr' })
      i++
      continue
    }

    const heading = line.match(/^(#{1,4})\s+(.+?)\s*#*\s*$/)
    if (heading) {
      blocks.push({
        kind: 'heading',
        level: heading[1].length as 1 | 2 | 3 | 4,
        text: heading[2],
      })
      i++
      continue
    }

    if (/^\s*>\s?/.test(line)) {
      const buf: string[] = []
      while (i < lines.length && /^\s*>\s?/.test(lines[i])) {
        buf.push(lines[i].replace(/^\s*>\s?/, ''))
        i++
      }
      blocks.push({ kind: 'quote', text: buf.join(' ') })
      continue
    }

    if (/^\s*[-*+]\s+/.test(line)) {
      const items: string[] = []
      while (i < lines.length && /^\s*[-*+]\s+/.test(lines[i])) {
        items.push(lines[i].replace(/^\s*[-*+]\s+/, ''))
        i++
      }
      blocks.push({ kind: 'ulist', items })
      continue
    }

    if (/^\s*\d+[.)]\s+/.test(line)) {
      const items: string[] = []
      while (i < lines.length && /^\s*\d+[.)]\s+/.test(lines[i])) {
        items.push(lines[i].replace(/^\s*\d+[.)]\s+/, ''))
        i++
      }
      blocks.push({ kind: 'olist', items })
      continue
    }

    const buf: string[] = [line]
    i++
    while (
      i < lines.length &&
      !/^\s*$/.test(lines[i]) &&
      !/^```/.test(lines[i]) &&
      !/^(#{1,4})\s+/.test(lines[i]) &&
      !/^\s*[-*+]\s+/.test(lines[i]) &&
      !/^\s*\d+[.)]\s+/.test(lines[i]) &&
      !/^\s*>/.test(lines[i]) &&
      !/^\s*(?:---|\*\*\*|___)\s*$/.test(lines[i])
    ) {
      buf.push(lines[i])
      i++
    }
    blocks.push({ kind: 'paragraph', text: buf.join(' ') })
  }

  return blocks
}

function renderBlock(block: Block, key: number): ReactNode {
  switch (block.kind) {
    case 'heading': {
      const cls = {
        1: 'text-base font-semibold mt-2 first:mt-0',
        2: 'text-sm font-semibold mt-2 first:mt-0',
        3: 'text-xs font-semibold mt-2 first:mt-0 text-foreground',
        4: 'text-xs font-medium mt-1.5 first:mt-0 text-foreground',
      }[block.level]
      const inner = renderInline(block.text)
      switch (block.level) {
        case 1:
          return <h1 key={key} className={cls}>{inner}</h1>
        case 2:
          return <h2 key={key} className={cls}>{inner}</h2>
        case 3:
          return <h3 key={key} className={cls}>{inner}</h3>
        case 4:
          return <h4 key={key} className={cls}>{inner}</h4>
      }
      return null
    }
    case 'paragraph':
      return (
        <p key={key} className="leading-relaxed">
          {renderInline(block.text)}
        </p>
      )
    case 'ulist':
      return (
        <ul key={key} className="list-disc space-y-1 pl-5">
          {block.items.map((it, j) => (
            <li key={j} className="leading-relaxed">
              {renderInline(it)}
            </li>
          ))}
        </ul>
      )
    case 'olist':
      return (
        <ol key={key} className="list-decimal space-y-1 pl-5">
          {block.items.map((it, j) => (
            <li key={j} className="leading-relaxed">
              {renderInline(it)}
            </li>
          ))}
        </ol>
      )
    case 'quote':
      return (
        <blockquote
          key={key}
          className="border-l-2 border-border pl-3 italic text-muted-foreground"
        >
          {renderInline(block.text)}
        </blockquote>
      )
    case 'code':
      return (
        <pre
          key={key}
          className="overflow-x-auto rounded-md border bg-muted/60 px-3 py-2 font-mono text-[11px] leading-relaxed"
        >
          <code>{block.content}</code>
        </pre>
      )
    case 'hr':
      return <hr key={key} className="border-border" />
  }
}

/**
 * Inline format: the scanner matches by priority at each position inline code / link / bold / italic,
 * whoever hits first consumes first.inline code has the highest priority, preventing inner content from being bolded/swallowed by the italic rule.
 */
function renderInline(text: string): ReactNode[] {
  const nodes: ReactNode[] = []
  let key = 0

  const patterns: Array<{ re: RegExp; render: (m: RegExpExecArray) => ReactNode }> = [
    {
      re: /`([^`]+)`/y,
      render: (m) => (
        <code
          key={key++}
          className="rounded bg-muted px-1 py-0.5 font-mono text-[11px] text-foreground"
        >
          {m[1]}
        </code>
      ),
    },
    {
      re: /\[([^\]]+)\]\(([^)\s]+)\)/y,
      render: (m) => (
        <a
          key={key++}
          href={m[2]}
          target="_blank"
          rel="noreferrer"
          className="underline decoration-dotted underline-offset-2 hover:decoration-solid"
        >
          {m[1]}
        </a>
      ),
    },
    {
      re: /\*\*([^*]+)\*\*/y,
      render: (m) => (
        <strong key={key++} className="font-semibold text-foreground">
          {m[1]}
        </strong>
      ),
    },
    {
      re: /\*([^*]+)\*/y,
      render: (m) => (
        <em key={key++} className="italic">
          {m[1]}
        </em>
      ),
    },
  ]

  let plainStart = 0
  let i = 0
  while (i < text.length) {
    let matched = false
    for (const p of patterns) {
      p.re.lastIndex = i
      const m = p.re.exec(text)
      if (m && m.index === i) {
        if (plainStart < i) {
          nodes.push(<span key={key++}>{text.slice(plainStart, i)}</span>)
        }
        nodes.push(p.render(m))
        i = m.index + m[0].length
        plainStart = i
        matched = true
        break
      }
    }
    if (!matched) i++
  }
  if (plainStart < text.length) {
    nodes.push(<span key={key++}>{text.slice(plainStart)}</span>)
  }
  return nodes
}
