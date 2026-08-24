import js from '@eslint/js'
import eslintConfigPrettier from 'eslint-config-prettier'
import pluginVue from 'eslint-plugin-vue'
import globals from 'globals'
import tseslint from 'typescript-eslint'

const typedParserOptions = {
  projectService: true,
  tsconfigRootDir: import.meta.dirname,
}

export default tseslint.config(
  {
    ignores: [
      'coverage/**',
      'dist/**',
      'node_modules/**',
      'package-lock.json',
      'playwright-report/**',
      'test-results/**',
    ],
  },
  {
    files: ['**/*.{ts,mts}'],
    extends: [js.configs.recommended, ...tseslint.configs.strictTypeChecked],
    languageOptions: {
      parserOptions: typedParserOptions,
    },
  },
  {
    files: ['**/*.vue'],
    extends: [
      js.configs.recommended,
      ...tseslint.configs.strictTypeChecked,
      ...pluginVue.configs['flat/recommended'],
    ],
    languageOptions: {
      parserOptions: {
        ...typedParserOptions,
        extraFileExtensions: ['.vue'],
        parser: tseslint.parser,
      },
    },
    rules: {
      // Route-level pages intentionally use concise resource names such as Items and Users.
      'vue/multi-word-component-names': 'off',
    },
  },
  {
    files: ['src/**/*.{ts,vue}'],
    languageOptions: {
      globals: globals.browser,
    },
  },
  {
    files: ['e2e/**/*.ts', 'tests/**/*.ts'],
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
  },
  {
    files: ['**/*.{js,mjs}'],
    extends: [js.configs.recommended],
    languageOptions: {
      globals: globals.node,
    },
  },
  eslintConfigPrettier,
)
