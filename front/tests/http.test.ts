import { afterEach, describe, expect, it, vi } from 'vitest'

import { ApiError, ClientError, messageFromError, requestJson } from '@/services/http'

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('requestJson', () => {
  it('serializes JSON requests and returns successful JSON responses', async () => {
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockResolvedValue(new Response(JSON.stringify({ value: 'ok' }), { status: 200 }))
    vi.stubGlobal('fetch', fetchMock)

    await expect(
      requestJson<{ value: string }>('/api/test', {
        body: { title: 'Example' },
        headers: { 'x-client': 'frontend' },
        method: 'POST',
      }),
    ).resolves.toEqual({ value: 'ok' })

    expect(fetchMock).toHaveBeenCalledOnce()
    const [path, init] = fetchMock.mock.calls[0] ?? []
    expect(path).toBe('/api/test')
    expect(init?.method).toBe('POST')
    expect(init?.body).toBe('{"title":"Example"}')
    expect(new Headers(init?.headers).get('content-type')).toBe('application/json')
    expect(new Headers(init?.headers).get('x-client')).toBe('frontend')
  })

  it('prefers the JSON request ID and displays it for operational failures', async () => {
    stubResponse(
      new Response(
        JSON.stringify({
          error: 'internal_error',
          message: 'internal server error',
          request_id: 'body-request-id',
        }),
        {
          status: 500,
          headers: { 'x-request-id': 'header-request-id' },
        },
      ),
    )

    const error = await captureApiError()
    expect(error).toMatchObject({
      code: 'internal_error',
      requestId: 'body-request-id',
      status: 500,
    })
    expect(messageFromError(error, 'fallback')).toBe(
      'internal server error (Reference: body-request-id)',
    )
  })

  it('uses the response header without displaying references for ordinary 4xx errors', async () => {
    stubResponse(
      new Response(JSON.stringify({ error: 'not_found', message: 'Record was not found' }), {
        status: 404,
        headers: { 'x-request-id': 'header-request-id' },
      }),
    )

    const error = await captureApiError()
    expect(error).toMatchObject({
      code: 'not_found',
      requestId: 'header-request-id',
      status: 404,
    })
    expect(messageFromError(error, 'fallback')).toBe('Record was not found')
  })

  it('normalizes non-JSON error bodies and keeps the response request ID', async () => {
    stubResponse(
      new Response('not-json', {
        status: 502,
        headers: { 'x-request-id': 'gateway-request-id' },
      }),
    )

    const error = await captureApiError()
    expect(error).toMatchObject({
      code: 'request_failed',
      message: 'Request failed with status 502',
      requestId: 'gateway-request-id',
      status: 502,
    })
    expect(messageFromError(error, 'fallback')).toBe(
      'Request failed with status 502 (Reference: gateway-request-id)',
    )
  })

  it('displays a request reference for request timeouts', async () => {
    stubResponse(
      new Response(
        JSON.stringify({
          error: 'request_timeout',
          message: 'request timed out',
          request_id: 'timeout-request-id',
        }),
        { status: 408 },
      ),
    )

    const error = await captureApiError()
    expect(messageFromError(error, 'fallback')).toBe(
      'request timed out (Reference: timeout-request-id)',
    )
  })

  it('normalizes invalid and empty successful responses', async () => {
    stubResponse(new Response('not-json', { status: 200 }))
    const invalid = await captureClientError()
    expect(invalid).toMatchObject({
      kind: 'invalid_response',
      message: 'The service returned an invalid response',
    })
    expect(invalid.cause).toBeInstanceOf(SyntaxError)

    stubResponse(new Response(null, { status: 204 }))
    await expect(requestJson('/api/test')).rejects.toMatchObject({
      kind: 'invalid_response',
      message: 'The service returned an invalid response',
    })
  })

  it('normalizes fetch and response-stream failures while retaining their causes', async () => {
    const fetchFailure = new TypeError('browser-specific fetch failure')
    vi.stubGlobal('fetch', vi.fn<typeof fetch>().mockRejectedValue(fetchFailure))
    const network = await captureClientError()
    expect(network).toMatchObject({
      cause: fetchFailure,
      kind: 'network',
      message: 'Unable to reach the service',
    })

    const streamFailure = new TypeError('connection reset while reading')
    vi.stubGlobal(
      'fetch',
      vi.fn<typeof fetch>().mockResolvedValue({
        ok: true,
        text: vi.fn().mockRejectedValue(streamFailure),
      } as unknown as Response),
    )
    await expect(requestJson('/api/test')).rejects.toMatchObject({
      cause: streamFailure,
      kind: 'network',
      message: 'Unable to reach the service',
    })
  })
})

describe('messageFromError', () => {
  it('uses ordinary Error messages and falls back for unknown values', () => {
    expect(messageFromError(new Error('known failure'), 'fallback')).toBe('known failure')
    expect(messageFromError({ reason: 'unknown' }, 'fallback')).toBe('fallback')
  })
})

function stubResponse(response: Response) {
  vi.stubGlobal('fetch', vi.fn<typeof fetch>().mockResolvedValue(response))
}

async function captureApiError(): Promise<ApiError> {
  try {
    await requestJson('/api/test')
  } catch (source) {
    if (source instanceof ApiError) {
      return source
    }
    throw source
  }
  throw new Error('request should fail')
}

async function captureClientError(): Promise<ClientError> {
  try {
    await requestJson('/api/test')
  } catch (source) {
    if (source instanceof ClientError) {
      return source
    }
    throw source
  }
  throw new Error('request should fail')
}
