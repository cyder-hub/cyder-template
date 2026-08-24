import { afterEach, describe, expect, it, vi } from 'vitest'

import { createItem, deleteItem, listItems } from '@/services/items'
import { createUser, deleteUser, listUsers } from '@/services/users'

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('resource services', () => {
  it('uses the item collection and encoded item paths', async () => {
    const fetchMock = jsonFetch()

    await listItems()
    await createItem({ completed: true, description: 'Details', title: 'Example' })
    await deleteItem('item/with space')

    expect(fetchMock).toHaveBeenNthCalledWith(1, '/api/items', expect.any(Object))
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      '/api/items',
      expect.objectContaining({ method: 'POST' }),
    )
    expect(fetchMock).toHaveBeenNthCalledWith(
      3,
      '/api/items/item%2Fwith%20space',
      expect.objectContaining({ method: 'DELETE' }),
    )
  })

  it('uses the user collection and encoded user paths', async () => {
    const fetchMock = jsonFetch()

    await listUsers()
    await createUser({ active: false, email: 'user@example.com', name: 'Example User' })
    await deleteUser('user/with space')

    expect(fetchMock).toHaveBeenNthCalledWith(1, '/api/users', expect.any(Object))
    expect(fetchMock).toHaveBeenNthCalledWith(
      2,
      '/api/users',
      expect.objectContaining({ method: 'POST' }),
    )
    expect(fetchMock).toHaveBeenNthCalledWith(
      3,
      '/api/users/user%2Fwith%20space',
      expect.objectContaining({ method: 'DELETE' }),
    )
  })
})

function jsonFetch() {
  const fetchMock = vi
    .fn<typeof fetch>()
    .mockImplementation(async () =>
      Promise.resolve(new Response('{}', { headers: { 'content-type': 'application/json' } })),
    )
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}
