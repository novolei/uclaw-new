/**
 * xml-utils — Shared helpers for the Office parsers.
 *
 * Ports Proma v0.9.27's pure-XML helpers (`file-preview-service.ts:150-225`)
 * to the browser, replacing `adm-zip` with `jszip` (async API).
 *
 * Why @xmldom/xmldom instead of DOMParser?
 * The browser's native DOMParser auto-handles XML namespaces, but emits
 * different tag names depending on the document (e.g. `c:sld` vs `p:sld`).
 * Proma walks XLSX/PPTX trees by LOCAL name only, which native DOMParser
 * doesn't expose cleanly. @xmldom/xmldom gives us `Element.localName` + raw
 * namespace prefixes, matching Proma's tree-walking logic verbatim.
 */

import { DOMParser } from '@xmldom/xmldom'
import type JSZip from 'jszip'

export function parseXml(xml: string): Document {
  // suppress error/warning console spam from @xmldom on malformed XML
  return new DOMParser({
    errorHandler: { warning: () => {}, error: () => {}, fatalError: () => {} },
  }).parseFromString(xml, 'text/xml') as unknown as Document
}

/** All descendant elements with the given local name (namespace-agnostic). */
export function getElementsByLocalName(root: Node, localName: string): Element[] {
  const out: Element[] = []
  const walk = (node: Node) => {
    if (node.nodeType === 1 /* ELEMENT_NODE */) {
      const el = node as Element
      if (el.localName === localName) out.push(el)
    }
    for (let i = 0; i < node.childNodes.length; i++) {
      walk(node.childNodes[i]!)
    }
  }
  walk(root)
  return out
}

/** Direct children only (not grandchildren) with the given local name. */
export function getDirectChildElementsByLocalName(
  root: Element | Document,
  localName: string,
): Element[] {
  const out: Element[] = []
  for (let i = 0; i < root.childNodes.length; i++) {
    const node = root.childNodes[i]!
    if (node.nodeType === 1 && (node as Element).localName === localName) {
      out.push(node as Element)
    }
  }
  return out
}

/** Concatenated text of the first descendant element with the given local name. */
export function getFirstTextByLocalName(root: Element, localName: string): string {
  for (let i = 0; i < root.childNodes.length; i++) {
    const node = root.childNodes[i]!
    if (node.nodeType === 1 && (node as Element).localName === localName) {
      return (node.textContent ?? '').trim()
    }
  }
  // Fall through to descendant search if no direct child match.
  const found = getElementsByLocalName(root, localName)[0]
  return found ? (found.textContent ?? '').trim() : ''
}

/** Read a file inside the zip as utf-8 text. Returns null if the entry is missing. */
export async function readZipText(zip: JSZip, path: string): Promise<string | null> {
  const file = zip.file(path)
  if (!file) return null
  return file.async('string')
}

/** Normalize a relationship target relative to a base dir within the zip. */
export function normalizeZipTarget(baseDir: string, target: string): string {
  if (target.startsWith('/')) return target.slice(1)
  // ".." segments collapse against baseDir.
  const parts = `${baseDir}/${target}`.split('/')
  const stack: string[] = []
  for (const p of parts) {
    if (p === '' || p === '.') continue
    if (p === '..') stack.pop()
    else stack.push(p)
  }
  return stack.join('/')
}

/** Parse a `_rels/*.rels` file into a Map<rId, target-path>. */
export async function parseRelationships(
  zip: JSZip,
  relsPath: string,
  baseDir: string,
): Promise<Map<string, string>> {
  const out = new Map<string, string>()
  const xml = await readZipText(zip, relsPath)
  if (!xml) return out
  const doc = parseXml(xml)
  for (const rel of getElementsByLocalName(doc, 'Relationship')) {
    const id = rel.getAttribute('Id')
    const target = rel.getAttribute('Target')
    if (id && target) out.set(id, normalizeZipTarget(baseDir, target))
  }
  return out
}

/** Minimal HTML escape — same behavior as Proma's helper. */
export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;')
}
