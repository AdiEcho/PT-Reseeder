import { toast } from 'sonner'

const BASE_HEADERS: Record<string, string> = {
  'Content-Type': 'application/json',
}

const MUTATING_METHODS = new Set(['POST', 'PUT', 'PATCH', 'DELETE'])

interface RequestOptions extends Omit<RequestInit, 'body'> {
  body?: unknown
}

export async function apiClient<T = unknown>(
  path: string,
  options: RequestOptions = {},
): Promise<T> {
  const { body, headers: extraHeaders, method = 'GET', ...rest } = options

  const headers: Record<string, string> = {
    ...BASE_HEADERS,
    ...(extraHeaders as Record<string, string>),
  }

  // Add CSRF-like header for mutating requests
  if (MUTATING_METHODS.has(method.toUpperCase())) {
    headers['X-PT-Reseeder'] = '1'
  }

  const response = await fetch(path, {
    method,
    headers,
    credentials: 'include',
    body: body != null ? JSON.stringify(body) : undefined,
    ...rest,
  })

  if (response.status === 401) {
    // Clear auth state and redirect to login
    window.location.href = '/login'
    throw new Error('Unauthorized')
  }

  if (response.status >= 500) {
    const msg = await response.text().catch(() => 'Internal Server Error')
    toast.error(`Server Error: ${msg}`)
    throw new Error(msg)
  }

  if (!response.ok) {
    const msg = await response.text().catch(() => response.statusText)
    throw new Error(msg)
  }

  // Return empty for 204
  if (response.status === 204) {
    return undefined as T
  }

  return response.json() as Promise<T>
}

// Convenience helpers
export const api = {
  get: <T = unknown>(path: string, opts?: RequestOptions) =>
    apiClient<T>(path, { ...opts, method: 'GET' }),

  post: <T = unknown>(path: string, body?: unknown, opts?: RequestOptions) =>
    apiClient<T>(path, { ...opts, method: 'POST', body }),

  put: <T = unknown>(path: string, body?: unknown, opts?: RequestOptions) =>
    apiClient<T>(path, { ...opts, method: 'PUT', body }),

  patch: <T = unknown>(path: string, body?: unknown, opts?: RequestOptions) =>
    apiClient<T>(path, { ...opts, method: 'PATCH', body }),

  delete: <T = unknown>(path: string, opts?: RequestOptions) =>
    apiClient<T>(path, { ...opts, method: 'DELETE' }),
}
