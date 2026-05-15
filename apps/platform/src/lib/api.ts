export function fetchApi<T>(path: string): Promise<T> {
  return fetch(path).then((r) => r.json())
}
