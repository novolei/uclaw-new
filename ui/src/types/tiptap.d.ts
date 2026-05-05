// [PLACEHOLDER] @tiptap type declarations for mention-suggestions
declare module '@tiptap/react' {
  export class ReactRenderer<T = any> {
    element: HTMLElement
    ref: T | null
    constructor(component: any, options?: any)
    updateProps(props: any): void
    destroy(): void
  }
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
