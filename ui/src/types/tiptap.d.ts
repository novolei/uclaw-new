// [PLACEHOLDER] @tiptap type declarations for mention-suggestions
declare module '@tiptap/react' {
  export class ReactRenderer<T = any> {
    element: HTMLElement
    ref: T | null
    constructor(component: any, options?: any)
    updateProps(props: any): void
    destroy(): void
  }
  // Minimal stubs for MarkdownRichEditor (W4d Task 13). The real types are
  // in @tiptap/react/dist/index.d.ts but this ambient module override blocks
  // them — extend here until the PLACEHOLDER is replaced with real TipTap usage.
  export interface EditorOptions {
    extensions?: any[]
    content?: string
    onUpdate?: (props: { editor: Editor }) => void
    [key: string]: any
  }
  export interface Editor {
    getHTML(): string
    destroy(): void
    [key: string]: any
  }
  export function useEditor(options: EditorOptions & { immediatelyRender?: boolean }, deps?: any[]): Editor
  export const EditorContent: React.ComponentType<{ editor: Editor | null; [key: string]: any }>
}

declare module '@tiptap/suggestion' {
  export interface SuggestionOptions<T = any> {
    editor: any
    char?: string
    allowSpaces?: boolean
    items?: (props: { query: string; editor: any }) => Promise<T[]> | T[]
    render?: () => {
      onStart?: (props: any) => void
      onUpdate?: (props: any) => void
      onKeyDown?: (props: any) => boolean
      onExit?: () => void
    }
    [key: string]: unknown
  }
}
