import { API_BASE, requestJson } from '@/services/http'

export interface User {
  id: number
  name: string
  email: string
  active: boolean
  created_at: number
  updated_at: number
}

export interface CreateUserRequest {
  name: string
  email: string
  active?: boolean
}

const USERS_PATH = `${API_BASE}/users`

export function listUsers(): Promise<User[]> {
  return requestJson<User[]>(USERS_PATH)
}

export function createUser(input: CreateUserRequest): Promise<User> {
  return requestJson<User>(USERS_PATH, {
    method: 'POST',
    body: input,
  })
}

export function deleteUser(id: number): Promise<{ deleted: boolean }> {
  return requestJson<{ deleted: boolean }>(`${USERS_PATH}/${id}`, {
    method: 'DELETE',
  })
}
