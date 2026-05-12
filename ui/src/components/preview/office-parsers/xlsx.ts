/**
 * xlsx — Convert a .xlsx file buffer to themed HTML.
 *
 * Pure-JS port of Proma v0.9.27's `convertXlsxToHtml`
 * (apps/electron/src/main/lib/file-preview-service.ts:396-449). Walks the
 * workbook XML, shared-strings table, and per-sheet rows using JSZip +
 * @xmldom/xmldom.
 *
 * Limits (match Proma):
 *   - MAX_XLSX_SHEETS = 8
 *   - MAX_XLSX_ROWS   = 100
 *   - MAX_XLSX_COLUMNS = 40
 *
 * Output HTML is class-tagged for the .office-* rules in globals.css.
 */

import JSZip from 'jszip'
import {
  escapeHtml,
  getDirectChildElementsByLocalName,
  getElementsByLocalName,
  getFirstTextByLocalName,
  parseRelationships,
  parseXml,
  readZipText,
} from './xml-utils'

const MAX_XLSX_SHEETS = 8
const MAX_XLSX_ROWS = 100
const MAX_XLSX_COLUMNS = 40

export interface XlsxResult {
  html: string
  /** Plain-text fallback (joined by `\n`) for accessibility / copy. */
  text: string
}

// ---- Shared strings -------------------------------------------------------

async function parseSharedStrings(zip: JSZip): Promise<string[]> {
  const xml = await readZipText(zip, 'xl/sharedStrings.xml')
  if (!xml) return []
  const doc = parseXml(xml)
  return getElementsByLocalName(doc, 'si').map((si) => {
    return getElementsByLocalName(si, 't')
      .map((t) => t.textContent ?? '')
      .join('')
  })
}

// ---- Date style detection -------------------------------------------------

function isDateNumFmtId(id: number): boolean {
  // Standard Excel format IDs that represent dates / times.
  return (id >= 14 && id <= 22) || (id >= 27 && id <= 36) || (id >= 45 && id <= 47)
}

function isDateFormatCode(code: string): boolean {
  const upper = code.toUpperCase()
  if (/AM\/PM|A\/P/.test(upper)) return true
  return /[YMDHS]/i.test(upper)
}

async function parseXlsxDateStyleIndexes(zip: JSZip): Promise<Set<number>> {
  const out = new Set<number>()
  const stylesXml = await readZipText(zip, 'xl/styles.xml')
  if (!stylesXml) return out
  const doc = parseXml(stylesXml)
  const customFormats = new Map<number, string>()
  for (const numFmt of getElementsByLocalName(doc, 'numFmt')) {
    const id = Number(numFmt.getAttribute('numFmtId'))
    const code = numFmt.getAttribute('formatCode') ?? ''
    if (Number.isFinite(id) && code) customFormats.set(id, code)
  }
  const cellXfs = getElementsByLocalName(doc, 'cellXfs')[0]
  if (!cellXfs) return out
  getDirectChildElementsByLocalName(cellXfs, 'xf').forEach((xf, index) => {
    const numFmtId = Number(xf.getAttribute('numFmtId'))
    if (!Number.isFinite(numFmtId)) return
    const customCode = customFormats.get(numFmtId)
    if (isDateNumFmtId(numFmtId) || (customCode && isDateFormatCode(customCode))) {
      out.add(index)
    }
  })
  return out
}

function formatExcelSerialDate(raw: string): string {
  const serial = Number(raw)
  if (!Number.isFinite(serial)) return raw
  // 25569 = days from 1900-01-00 to 1970-01-01.
  const ms = Math.round((serial - 25569) * 86400 * 1000)
  const date = new Date(ms)
  if (Number.isNaN(date.getTime())) return raw
  const year = date.getUTCFullYear()
  if (year < 1900 || year > 9999) return raw
  const pad = (n: number) => String(n).padStart(2, '0')
  const datePart = `${year}-${pad(date.getUTCMonth() + 1)}-${pad(date.getUTCDate())}`
  const hasTime = Math.abs(serial - Math.floor(serial)) > 0.000001
  if (!hasTime) return datePart
  return `${datePart} ${pad(date.getUTCHours())}:${pad(date.getUTCMinutes())}`
}

// ---- Cell ref helpers -----------------------------------------------------

function columnIndexFromCellRef(cellRef: string): number {
  const letters = cellRef.match(/[A-Za-z]+/)?.[0]?.toUpperCase()
  if (!letters) return 0
  let index = 0
  for (const char of letters) {
    index = index * 26 + (char.charCodeAt(0) - 64)
  }
  return Math.max(0, index - 1)
}

function columnNameFromIndex(index: number): string {
  let value = index + 1
  let name = ''
  while (value > 0) {
    const rem = (value - 1) % 26
    name = String.fromCharCode(65 + rem) + name
    value = Math.floor((value - 1) / 26)
  }
  return name
}

function getXlsxCellText(
  cell: Element,
  sharedStrings: string[],
  dateStyleIndexes: Set<number>,
): string {
  const type = cell.getAttribute('t')
  if (type === 'inlineStr') {
    return getElementsByLocalName(cell, 't')
      .map((node) => node.textContent ?? '')
      .join('')
  }
  const value = getFirstTextByLocalName(cell, 'v')
  if (!value) return ''
  if (type === 's') {
    const idx = Number(value)
    return Number.isInteger(idx) ? sharedStrings[idx] ?? '' : ''
  }
  if (type === 'b') return value === '1' ? 'TRUE' : 'FALSE'
  const styleIndex = Number(cell.getAttribute('s'))
  if (!type && Number.isInteger(styleIndex) && dateStyleIndexes.has(styleIndex)) {
    return formatExcelSerialDate(value)
  }
  return value
}

// ---- Sheet rows -----------------------------------------------------------

interface SheetRows {
  rows: string[][]
  truncatedRows: boolean
  truncatedColumns: boolean
}

async function parseXlsxSheetRows(
  zip: JSZip,
  sheetPath: string,
  sharedStrings: string[],
  dateStyleIndexes: Set<number>,
): Promise<SheetRows> {
  const xml = await readZipText(zip, sheetPath)
  if (!xml) return { rows: [], truncatedRows: false, truncatedColumns: false }
  const doc = parseXml(xml)
  const rows: string[][] = []
  let truncatedRows = false
  let truncatedColumns = false

  for (const row of getElementsByLocalName(doc, 'row')) {
    if (rows.length >= MAX_XLSX_ROWS) {
      truncatedRows = true
      break
    }
    const values: string[] = []
    for (const cell of getDirectChildElementsByLocalName(row, 'c')) {
      const ref = cell.getAttribute('r') ?? ''
      const col = columnIndexFromCellRef(ref)
      if (col >= MAX_XLSX_COLUMNS) {
        truncatedColumns = true
        continue
      }
      values[col] = getXlsxCellText(cell, sharedStrings, dateStyleIndexes)
    }
    while (values.length > 0 && !values[values.length - 1]) values.pop()
    if (values.some((v) => v.trim().length > 0)) rows.push(values)
  }
  return { rows, truncatedRows, truncatedColumns }
}

// ---- HTML emission --------------------------------------------------------

function renderXlsxTable(rows: string[][]): string {
  if (rows.length === 0) {
    return '<div class="office-empty">这个工作表没有可预览的数据</div>'
  }
  const cols = Math.max(...rows.map((r) => r.length), 1)
  const headerCells = Array.from(
    { length: cols },
    (_, i) => `<th>${escapeHtml(columnNameFromIndex(i))}</th>`,
  ).join('')
  const bodyRows = rows
    .map((row, rowIdx) => {
      const cells = Array.from(
        { length: cols },
        (_, i) => `<td>${escapeHtml(row[i] ?? '')}</td>`,
      ).join('')
      return `<tr><th class="office-row-heading">${rowIdx + 1}</th>${cells}</tr>`
    })
    .join('')
  return `<div class="office-table-wrap"><table><thead><tr><th></th>${headerCells}</tr></thead><tbody>${bodyRows}</tbody></table></div>`
}

// ---- Entry point ----------------------------------------------------------

export async function convertXlsxToHtml(bytes: Uint8Array, filename: string): Promise<XlsxResult> {
  const zip = await JSZip.loadAsync(bytes)
  const workbookXml = await readZipText(zip, 'xl/workbook.xml')
  if (!workbookXml) throw new Error('Invalid XLSX: workbook.xml missing')

  const workbookDoc = parseXml(workbookXml)
  const relationships = await parseRelationships(zip, 'xl/_rels/workbook.xml.rels', 'xl')
  const sharedStrings = await parseSharedStrings(zip)
  const dateStyleIndexes = await parseXlsxDateStyleIndexes(zip)
  const sheets = getElementsByLocalName(workbookDoc, 'sheet')

  let truncatedRows = false
  let truncatedColumns = false
  const textParts: string[] = []
  const htmlParts: string[] = []

  // XML namespaces vary — get r:id OR id.
  for (const sheet of sheets.slice(0, MAX_XLSX_SHEETS)) {
    const name = sheet.getAttribute('name') || `Sheet ${htmlParts.length + 1}`
    const relId = sheet.getAttribute('r:id') ?? sheet.getAttribute('id')
    const sheetPath = relId ? relationships.get(relId) : undefined
    if (!sheetPath) continue
    const parsed = await parseXlsxSheetRows(zip, sheetPath, sharedStrings, dateStyleIndexes)
    truncatedRows ||= parsed.truncatedRows
    truncatedColumns ||= parsed.truncatedColumns
    textParts.push(`[${name}]`)
    textParts.push(...parsed.rows.map((r) => r.join('\t')))
    htmlParts.push(
      `<section class="office-sheet"><h3>${escapeHtml(name)}</h3>${renderXlsxTable(parsed.rows)}</section>`,
    )
  }

  if (htmlParts.length === 0) {
    throw new Error('Invalid XLSX: no worksheet data resolved')
  }

  const truncatedSheets = sheets.length > MAX_XLSX_SHEETS
  const notices: string[] = []
  if (truncatedSheets) notices.push(`仅显示前 ${MAX_XLSX_SHEETS} 个工作表`)
  if (truncatedRows) notices.push(`每个工作表最多显示 ${MAX_XLSX_ROWS} 行`)
  if (truncatedColumns) notices.push(`每行最多显示 ${MAX_XLSX_COLUMNS} 列`)
  const noticeHtml =
    notices.length > 0
      ? `<div class="office-preview-notice">${escapeHtml(notices.join('，'))}</div>`
      : ''

  const html = `<div class="office-preview office-preview-spreadsheet"><div class="office-preview-title">${escapeHtml(filename)}</div>${noticeHtml}${htmlParts.join('')}</div>`
  return { html, text: textParts.join('\n').trim() }
}
