// Type declaration for mermaid (dynamically imported)
declare module 'mermaid' {
  interface MermaidConfig {
    startOnLoad?: boolean
    theme?: string
    securityLevel?: string
    fontFamily?: string
    [key: string]: unknown
  }

  interface RenderResult {
    svg: string
    bindFunctions?: (element: HTMLElement) => void
  }

  interface Mermaid {
    initialize: (config: MermaidConfig) => void
    render: (id: string, definition: string) => Promise<RenderResult>
    parse: (definition: string) => Promise<boolean>
  }

  const mermaid: Mermaid
  export default mermaid
}
