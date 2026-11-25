import { api } from '../api/client.ts'

/** Downloads the records of `scope` as the file the server names. */
export async function saveFile(scope: string): Promise<void> {
  const { file, name } = await api.download(scope)
  const url = URL.createObjectURL(file)
  const a = document.createElement('a')
  a.href = url
  a.download = name
  a.click()
  URL.revokeObjectURL(url)
}
