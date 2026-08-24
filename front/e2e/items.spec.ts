import { expect, test } from '@playwright/test'

test('creates and deletes an example item through the complete image', async ({ page }) => {
  const title = `Playwright item ${String(Date.now())}`

  await page.goto('/items')
  await expect(page.getByRole('heading', { level: 1, name: 'Items' })).toBeVisible()
  await page.getByLabel('Title').fill(title)
  await page.getByLabel('Description').fill('Created by the container E2E contract')
  await page.getByRole('button', { name: 'Create item' }).click()

  const row = page.getByRole('row').filter({ hasText: title })
  await expect(row).toBeVisible()
  await row.getByRole('button', { name: 'Delete' }).click()
  await expect(row).toHaveCount(0)
})
