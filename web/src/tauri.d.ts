/** Type stubs for @tauri-apps/api — only available at runtime inside Tauri desktop shell. */
declare module '@tauri-apps/api/core' {
  export function invoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T>
}
