/**
 * pptx — Convert a .pptx file buffer to themed HTML.
 *
 * Pure-JS port of Proma v0.9.27's `convertPptxToHtml`
 * (apps/electron/src/main/lib/file-preview-service.ts:483-520). Walks
 * the presentation.xml + per-slide XML to extract text only (no shapes,
 * no images, no transitions — it's a preview, not a Keynote replacement).
 *
 * Output HTML is class-tagged for the .office-slide / .office-empty rules
 * in globals.css.
 */

import JSZip from 'jszip'
import {
  escapeHtml,
  getElementsByLocalName,
  parseRelationships,
  parseXml,
  readZipText,
} from './xml-utils'

const MAX_PPTX_SLIDES = 80

export interface PptxResult {
  html: string
  /** Plain-text fallback (joined by \n) for accessibility / copy. */
  text: string
}

async function getPptxSlidePaths(zip: JSZip): Promise<string[]> {
  const presentationXml = await readZipText(zip, 'ppt/presentation.xml')
  const relationships = await parseRelationships(zip, 'ppt/_rels/presentation.xml.rels', 'ppt')
  if (presentationXml) {
    const doc = parseXml(presentationXml)
    const paths = getElementsByLocalName(doc, 'sldId')
      .map((s) => s.getAttribute('r:id') ?? s.getAttribute('id'))
      .map((rid) => (rid ? relationships.get(rid) : undefined))
      .filter((p): p is string => Boolean(p))
    if (paths.length > 0) return paths
  }
  // Fallback: enumerate slides directly.
  const out: string[] = []
  zip.forEach((path) => {
    if (/^ppt\/slides\/slide\d+\.xml$/.test(path)) out.push(path)
  })
  return out.sort((a, b) => {
    const numA = Number(a.match(/slide(\d+)\.xml$/)?.[1] ?? 0)
    const numB = Number(b.match(/slide(\d+)\.xml$/)?.[1] ?? 0)
    return numA - numB
  })
}

async function getPptxSlideText(zip: JSZip, slidePath: string): Promise<string[]> {
  const xml = await readZipText(zip, slidePath)
  if (!xml) return []
  const doc = parseXml(xml)
  return getElementsByLocalName(doc, 'p')
    .map((paragraph) =>
      getElementsByLocalName(paragraph, 't')
        .map((textNode) => textNode.textContent ?? '')
        .join('')
        .trim(),
    )
    .filter(Boolean)
}

export async function convertPptxToHtml(bytes: Uint8Array, filename: string): Promise<PptxResult> {
  const zip = await JSZip.loadAsync(bytes)
  const slidePaths = await getPptxSlidePaths(zip)
  const visible = slidePaths.slice(0, MAX_PPTX_SLIDES)

  const textParts: string[] = []
  const slideHtmlParts: string[] = []

  for (let i = 0; i < visible.length; i++) {
    const slidePath = visible[i]!
    const lines = await getPptxSlideText(zip, slidePath)
    textParts.push(`幻灯片 ${i + 1}`)
    textParts.push(...lines)
    const title = lines[0] || '（无标题）'
    const body =
      lines.length > 1
        ? `<ul>${lines.slice(1).map((line) => `<li>${escapeHtml(line)}</li>`).join('')}</ul>`
        : '<div class="office-empty">这页没有更多可提取文本</div>'
    slideHtmlParts.push(
      `<section class="office-slide"><div class="office-slide-index">幻灯片 ${i + 1}</div><h3>${escapeHtml(title)}</h3>${body}</section>`,
    )
  }

  if (slideHtmlParts.length === 0) {
    throw new Error('Invalid PPTX: no slides resolved')
  }

  const noticeHtml =
    slidePaths.length > MAX_PPTX_SLIDES
      ? `<div class="office-preview-notice">${escapeHtml(`仅显示前 ${MAX_PPTX_SLIDES} 页幻灯片`)}</div>`
      : ''

  const html = `<div class="office-preview office-preview-presentation"><div class="office-preview-title">${escapeHtml(filename)}</div>${noticeHtml}${slideHtmlParts.join('')}</div>`
  return { html, text: textParts.join('\n').trim() }
}
