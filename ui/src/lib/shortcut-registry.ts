// [PLACEHOLDER] shortcut-registry — 待后续任务迁移

/** 获取当前平台的快捷键 */
export function getActiveAccelerator(_shortcutId: string): string {
  return ''
}

/** 获取快捷键的显示文本 */
export function getAcceleratorDisplay(accelerator: string): string {
  return accelerator || 'Esc'
}
