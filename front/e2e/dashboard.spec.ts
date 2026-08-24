import { expect, test } from '@playwright/test'

test('loads the dashboard and reports a healthy ready service', async ({ page }) => {
  await page.goto('/')

  await expect(page.getByRole('heading', { level: 1, name: 'Operator dashboard' })).toBeVisible()
  const status = page.getByRole('region', { name: 'Service status' })
  await expect(status.getByText('Ready', { exact: true })).toBeVisible()
  await expect(status.getByText('ok', { exact: true })).toBeVisible()
  await expect(status.getByText('connected', { exact: true })).toBeVisible()
})
