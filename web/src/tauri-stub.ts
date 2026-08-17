// Stub for @tauri-apps/api/core in web dev mode
export function invoke(_cmd: string, _args?: Record<string, unknown>): Promise<unknown> {
  return Promise.reject(new Error('Tauri not available in web mode'))
}
