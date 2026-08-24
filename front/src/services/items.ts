import { API_BASE, requestJson } from '@/services/http'

export interface Item {
  id: string
  title: string
  description: string
  completed: boolean
  created_at: number
  updated_at: number
}

export interface CreateItemRequest {
  title: string
  description?: string
  completed?: boolean
}

const ITEMS_PATH = `${API_BASE}/items`

export function listItems(): Promise<Item[]> {
  return requestJson<Item[]>(ITEMS_PATH)
}

export function createItem(input: CreateItemRequest): Promise<Item> {
  return requestJson<Item>(ITEMS_PATH, {
    method: 'POST',
    body: input,
  })
}

export function deleteItem(id: string): Promise<{ deleted: boolean }> {
  return requestJson<{ deleted: boolean }>(`${ITEMS_PATH}/${encodeURIComponent(id)}`, {
    method: 'DELETE',
  })
}
