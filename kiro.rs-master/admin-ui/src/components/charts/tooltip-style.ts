/**
 * threeitems recharts imagetotaluseof Tooltip style
 *
 * note:recharts of Tooltip label and item eachhasindependent style,
 * does not inherit contentStyle.color;must be set separately labelStyle / itemStyle.
 */
import type { CSSProperties } from 'react'

export const tooltipContentStyle: CSSProperties = {
  background: 'rgba(20,20,20,0.94)',
  border: '1px solid rgba(255,255,255,0.08)',
  borderRadius: 10,
  padding: '8px 12px',
  boxShadow: '0 8px 24px rgba(0,0,0,0.25)',
  fontSize: 12,
  color: '#fff',
}

export const tooltipLabelStyle: CSSProperties = {
  color: 'rgba(255,255,255,0.85)',
  fontWeight: 500,
  marginBottom: 4,
}

export const tooltipItemStyle: CSSProperties = {
  color: '#fff',
  padding: '2px 0',
}

export const tooltipCursorStyle = {
  fill: 'rgba(255,255,255,0.06)',
}
