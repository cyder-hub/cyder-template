import { flushPromises, mount } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import Users from '@/pages/Users.vue'
import { ApiError } from '@/services/http'
import { createUser, deleteUser, listUsers, type User } from '@/services/users'

vi.mock('@/services/users', () => ({
  createUser: vi.fn(),
  deleteUser: vi.fn(),
  listUsers: vi.fn(),
}))

const createUserMock = vi.mocked(createUser)
const deleteUserMock = vi.mocked(deleteUser)
const listUsersMock = vi.mocked(listUsers)
const exampleUser: User = {
  active: true,
  created_at: 1_786_800_000_000,
  email: 'user@example.com',
  id: '202',
  name: 'Example User',
  updated_at: 1_786_800_000_000,
}

beforeEach(() => {
  listUsersMock.mockResolvedValue([])
})

describe('Users', () => {
  it('validates required name and email fields', async () => {
    const wrapper = mount(Users)
    await flushPromises()

    await wrapper.get('form').trigger('submit')
    expect(wrapper.text()).toContain('Name is required')

    await wrapper.get('input[autocomplete="name"]').setValue('Example User')
    await wrapper.get('form').trigger('submit')
    expect(wrapper.text()).toContain('A valid email is required')

    await wrapper.get('input[autocomplete="email"]').setValue('invalid-email')
    await wrapper.get('form').trigger('submit')
    expect(wrapper.text()).toContain('A valid email is required')
    expect(createUserMock).not.toHaveBeenCalled()
  })

  it('creates an inactive user and resets the form', async () => {
    createUserMock.mockResolvedValue({ ...exampleUser, active: false })
    const wrapper = mount(Users)
    await flushPromises()

    await wrapper.get('input[autocomplete="name"]').setValue('  Example User  ')
    await wrapper.get('input[autocomplete="email"]').setValue('  user@example.com  ')
    const toggle = wrapper.findAll('button').find((candidate) => candidate.text() === 'Active')
    if (!toggle) {
      throw new Error('Active toggle not found')
    }
    await toggle.trigger('click')
    await wrapper.get('form').trigger('submit')
    await flushPromises()

    expect(createUserMock).toHaveBeenCalledWith({
      active: false,
      email: 'user@example.com',
      name: 'Example User',
    })
    expect(wrapper.text()).toContain('Example User')
    expect(wrapper.text()).toContain('1 total, 0 active')
    expect(wrapper.get<HTMLInputElement>('input[autocomplete="name"]').element.value).toBe('')
    expect(wrapper.get<HTMLInputElement>('input[autocomplete="email"]').element.value).toBe('')
  })

  it('removes users and reports delete errors without an operational reference', async () => {
    listUsersMock.mockResolvedValue([exampleUser])
    deleteUserMock.mockRejectedValueOnce(
      new ApiError(404, {
        error: 'not_found',
        message: 'User was not found',
        request_id: 'delete-request-id',
      }),
    )
    const wrapper = mount(Users)
    await flushPromises()

    await wrapper.get('tbody tr').get('button').trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('User was not found')
    expect(wrapper.text()).not.toContain('delete-request-id')
    expect(wrapper.text()).toContain('Example User')

    deleteUserMock.mockResolvedValueOnce({ deleted: true })
    await wrapper.get('tbody tr').get('button').trigger('click')
    await flushPromises()
    expect(deleteUserMock).toHaveBeenCalledWith('202')
    expect(wrapper.text()).not.toContain('Example User')
  })

  it('shows a stable create failure and restores the submit button', async () => {
    createUserMock.mockRejectedValue(new Error('Unable to save user'))
    const wrapper = mount(Users)
    await flushPromises()
    await wrapper.get('input[autocomplete="name"]').setValue('Example User')
    await wrapper.get('input[autocomplete="email"]').setValue('user@example.com')
    await wrapper.get('form').trigger('submit')
    await flushPromises()

    expect(wrapper.text()).toContain('Unable to save user')
    const submit = wrapper.findAll('button').find((candidate) => candidate.text() === 'Create user')
    expect(submit?.attributes('disabled')).toBeUndefined()
  })
})
