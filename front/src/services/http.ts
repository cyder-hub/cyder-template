export const API_BASE = '/api'

export interface ApiErrorBody {
  error: string
  message: string
}

export class ApiError extends Error {
  readonly status: number
  readonly code: string

  constructor(status: number, body: ApiErrorBody) {
    super(body.message)
    this.name = 'ApiError'
    this.status = status
    this.code = body.error
  }
}

export interface RequestOptions extends Omit<RequestInit, 'body'> {
  body?: unknown
}

export async function requestJson<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const { body, ...requestOptions } = options
  const headers = new Headers(requestOptions.headers)
  const init: RequestInit = {
    ...requestOptions,
    headers,
  }

  if (body !== undefined) {
    headers.set('content-type', 'application/json')
    init.body = JSON.stringify(body)
  }

  const response = await fetch(path, init)
  const text = await response.text()
  const data: unknown = text ? JSON.parse(text) : null

  if (!response.ok) {
    throw new ApiError(response.status, normalizeErrorBody(data, response.status))
  }

  return data as T
}

function normalizeErrorBody(data: unknown, status: number): ApiErrorBody {
  if (
    data &&
    typeof data === 'object' &&
    'error' in data &&
    'message' in data &&
    typeof data.error === 'string' &&
    typeof data.message === 'string'
  ) {
    return {
      error: data.error,
      message: data.message,
    }
  }

  return {
    error: 'request_failed',
    message: `Request failed with status ${status}`,
  }
}
