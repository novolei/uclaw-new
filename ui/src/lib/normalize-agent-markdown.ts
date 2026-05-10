/**
 * Insert missing newlines before markdown heading markers that follow content.
 *
 * Some LLMs (notably DeepSeek) emit headings inline without the required
 * leading newline: `prose### Heading` instead of `prose\n### Heading`.
 * react-markdown then renders the literal `###` chars as text instead of
 * a heading. This util inserts the newline.
 *
 * ## Pitfalls (history)
 *
 * Earlier versions had two bugs:
 *
 * 1. `([^\n])(\n?[-*] )` was used to also fix list markers; that regex
 *    matched `-- ` substrings inside markdown table separator rows
 *    (`| --- | --- |`), turning them into bullet lists and collapsing
 *    the whole table. Removed in PR #68.
 *
 * 2. `([^\n])(#{1,6} )` matched the FIRST `#` of `## heading` as the
 *    prefix capture, then consumed the second `#` as part of the
 *    heading marker. Result: `## heading` → `#\n# heading`, splitting
 *    every multi-`#` heading into a stray `#` line + a wrong-level
 *    heading. Visible in the UI as a row of empty `|` decoration bars
 *    (the H2 accent bar) above headings. Fixed by excluding `#` from
 *    the prefix character class: `([^\n#])(#{1,6} )`.
 *
 * Lines that look like table content (start with `|`) are passed
 * through unchanged so headings inside cells (rare) can't trigger
 * either rule.
 */
export function normalizeAgentMarkdown(text: string): string {
  return text
    .split('\n')
    .map((line) => {
      if (line.trimStart().startsWith('|')) return line
      // [^\n#] prefix prevents matching the leading '#' of multi-# headings.
      return line.replace(/([^\n#])(#{1,6} )/g, '$1\n$2')
    })
    .join('\n')
}
