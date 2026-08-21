import assert from 'node:assert/strict'
import test from 'node:test'

import { ApiError, messageFromError, requestJson } from '../src/services/http.ts'

test('uses the JSON request ID and displays it for operational failures', async () => {
  await withFetch(
    new Response(
      JSON.stringify({
        error: 'internal_error',
        message: 'internal server error',
        request_id: 'body-request-id',
      }),
      {
        status: 500,
        headers: {
          'content-type': 'application/json',
          'x-request-id': 'header-request-id',
        },
      },
    ),
    async () => {
      const error = await captureApiError()
      assert.equal(error.requestId, 'body-request-id')
      assert.equal(
        messageFromError(error, 'fallback'),
        'internal server error (Reference: body-request-id)',
      )
    },
  )
})

test('falls back to the response header without displaying references for ordinary 4xx', async () => {
  await withFetch(
    new Response('not-json', {
      status: 404,
      headers: { 'x-request-id': 'header-request-id' },
    }),
    async () => {
      const error = await captureApiError()
      assert.equal(error.requestId, 'header-request-id')
      assert.equal(error.code, 'request_failed')
      assert.equal(messageFromError(error, 'fallback'), 'Request failed with status 404')
    },
  )
})

test('displays a request reference for request timeouts', async () => {
  await withFetch(
    new Response(
      JSON.stringify({
        error: 'request_timeout',
        message: 'request timed out',
        request_id: 'timeout-request-id',
      }),
      { status: 408, headers: { 'content-type': 'application/json' } },
    ),
    async () => {
      const error = await captureApiError()
      assert.equal(
        messageFromError(error, 'fallback'),
        'request timed out (Reference: timeout-request-id)',
      )
    },
  )
})

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

async function withFetch(response: Response, run: () => Promise<void>) {
  const originalFetch = globalThis.fetch
  globalThis.fetch = async () => response
  try {
    await run()
  } finally {
    globalThis.fetch = originalFetch
  }
}
