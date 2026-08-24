import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { nextTick } from 'vue'

import Items from '@/pages/Items.vue'
import { ApiError, ClientError } from '@/services/http'
import { createItem, deleteItem, listItems, type Item } from '@/services/items'

vi.mock('@/services/items', () => ({
  createItem: vi.fn(),
  deleteItem: vi.fn(),
  listItems: vi.fn(),
}))

const createItemMock = vi.mocked(createItem)
const deleteItemMock = vi.mocked(deleteItem)
const listItemsMock = vi.mocked(listItems)
const exampleItem: Item = {
  completed: false,
  created_at: 1_786_800_000_000,
  description: 'Operational details',
  id: '101',
  title: 'Example item',
  updated_at: 1_786_800_000_000,
}

beforeEach(() => {
  listItemsMock.mockResolvedValue([])
})

describe('Items', () => {
  it('renders loading and empty states', async () => {
    const listing = deferred<Item[]>()
    listItemsMock.mockReturnValue(listing.promise)
    const wrapper = mount(Items)

    await nextTick()
    expect(wrapper.text()).toContain('Loading items')
    listing.resolve([])
    await flushPromises()
    expect(wrapper.text()).toContain('No items yet')
  })

  it('validates and creates an item from user input', async () => {
    createItemMock.mockResolvedValue(exampleItem)
    const wrapper = mount(Items)
    await flushPromises()

    await wrapper.get('form').trigger('submit')
    expect(wrapper.text()).toContain('Title is required')
    expect(createItemMock).not.toHaveBeenCalled()

    await wrapper.get('input').setValue('  Example item  ')
    await wrapper.get('textarea').setValue('  Operational details  ')
    const toggle = wrapper.findAll('button').find((candidate) => candidate.text() === 'Open')
    if (!toggle) {
      throw new Error('Open toggle not found')
    }
    await toggle.trigger('click')
    await wrapper.get('form').trigger('submit')
    await flushPromises()

    expect(createItemMock).toHaveBeenCalledWith({
      completed: true,
      description: 'Operational details',
      title: 'Example item',
    })
    expect(wrapper.text()).toContain('Example item')
    expect(wrapper.text()).toContain('1 total, 0 completed')
    expect(wrapper.get<HTMLInputElement>('input').element.value).toBe('')
    expect(wrapper.get<HTMLTextAreaElement>('textarea').element.value).toBe('')
  })

  it('shows stable create failures', async () => {
    createItemMock.mockRejectedValue(
      new ApiError(500, {
        error: 'internal_error',
        message: 'internal server error',
        request_id: 'create-request-id',
      }),
    )
    const wrapper = mount(Items)
    await flushPromises()
    await wrapper.get('input').setValue('Failed item')
    await wrapper.get('form').trigger('submit')
    await flushPromises()

    expect(wrapper.text()).toContain('internal server error (Reference: create-request-id)')
    const submit = wrapper.findAll('button').find((candidate) => candidate.text() === 'Create item')
    expect(submit?.attributes('disabled')).toBeUndefined()
  })

  it('removes a listed item and exposes the pending delete state', async () => {
    listItemsMock.mockResolvedValue([exampleItem])
    const deletion = deferred<{ deleted: boolean }>()
    deleteItemMock.mockReturnValue(deletion.promise)
    const wrapper = mount(Items)
    await flushPromises()

    const deleteButton = wrapper.get('tbody tr').get('button')
    await deleteButton.trigger('click')
    expect(deleteButton.attributes('disabled')).toBeDefined()
    expect(deleteButton.text()).toBe('Deleting')

    deletion.resolve({ deleted: true })
    await flushPromises()
    expect(deleteItemMock).toHaveBeenCalledWith('101')
    expect(wrapper.text()).not.toContain('Example item')
  })

  it('shows a stable load failure', async () => {
    listItemsMock.mockRejectedValue(new ClientError('network', 'Unable to reach the service'))
    const wrapper = mount(Items)
    await flushPromises()
    expect(wrapper.text()).toContain('Unable to reach the service')
  })
})

function deferred<T>() {
  let resolvePromise!: (value: T) => void
  const promise = new Promise<T>((resolve) => {
    resolvePromise = resolve
  })
  return { promise, resolve: resolvePromise }
}
