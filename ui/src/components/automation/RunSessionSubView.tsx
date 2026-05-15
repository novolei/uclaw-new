interface Props {
  sessionId: string
  onBack: () => void
}
export function RunSessionSubView({ sessionId, onBack }: Props) {
  return (
    <div>
      <button onClick={onBack}>← 动态</button>
      <div data-testid="run-session-stub">{sessionId}</div>
    </div>
  )
}
