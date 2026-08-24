export const API_BASE = '/api'

export interface ApiErrorBody {
  error: string
  message: string
  request_id: string | null
}

export type ClientErrorKind = 'invalid_response' | 'network'

export class ClientError extends Error {
  readonly kind: ClientErrorKind

  constructor(kind: ClientErrorKind, message: string, options?: ErrorOptions) {
    super(message, options)
    this.name = 'ClientError'
    this.kind = kind
  }
}

export class ApiError extends Error {
  readonly status: number
  readonly code: string
  readonly requestId: string | null

  constructor(status: number, body: ApiErrorBody) {
    super(body.message)
    this.name = 'ApiError'
    this.status = status
    this.code = body.error
    this.requestId = body.request_id
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

  let response: Response
  try {
    response = await fetch(path, init)
  } catch (source) {
    throw new ClientError('network', 'Unable to reach the service', { cause: source })
  }

  let text: string
  try {
    text = await response.text()
  } catch (source) {
    throw new ClientError('network', 'Unable to reach the service', { cause: source })
  }

  let data: unknown = null
  if (text) {
    try {
      data = JSON.parse(text)
    } catch (source) {
      if (response.ok) {
        throw new ClientError('invalid_response', 'The service returned an invalid response', {
          cause: source,
        })
      }
    }
  }

  if (!response.ok) {
    throw new ApiError(
      response.status,
      normalizeErrorBody(data, response.status, response.headers.get('x-request-id')),
    )
  }

  if (!text) {
    throw new ClientError('invalid_response', 'The service returned an invalid response')
  }

  return data as T
}

function normalizeErrorBody(
  data: unknown,
  status: number,
  responseRequestId: string | null,
): ApiErrorBody {
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
      request_id:
        'request_id' in data && typeof data.request_id === 'string'
          ? data.request_id
          : responseRequestId,
    }
  }

  return {
    error: 'request_failed',
    message: `Request failed with status ${String(status)}`,
    request_id: responseRequestId,
  }
}

export function messageFromError(source: unknown, fallback: string): string {
  if (source instanceof ApiError) {
    if ((source.status >= 500 || source.status === 408) && source.requestId) {
      return `${source.message} (Reference: ${source.requestId})`
    }
    return source.message
  }
  return source instanceof Error ? source.message : fallback
}
