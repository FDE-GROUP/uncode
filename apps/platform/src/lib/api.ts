export const API = 'http://127.0.0.1:3000'

export function fetchApi<T>(path: string): Promise<T> {
  return fetch(`${API}${path}`).then((r) => r.json())
}
