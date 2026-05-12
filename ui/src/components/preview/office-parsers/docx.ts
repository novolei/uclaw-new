// Thin wrapper around mammoth.convertToHtml for DOCX → HTML conversion
interface ConvertResult {
  html: string
  messages: { type: string; message: string }[]
}

export async function convertDocxToHtml(bytes: Uint8Array): Promise<ConvertResult> {
  const mammoth = await import('mammoth')
  const result = await mammoth.convertToHtml({ arrayBuffer: bytes.buffer as ArrayBuffer })
  return {
    html: result.value,
    messages: (result.messages ?? []).map((m: { type: string; message: string }) => ({
      type: m.type,
      message: m.message,
    })),
  }
}
