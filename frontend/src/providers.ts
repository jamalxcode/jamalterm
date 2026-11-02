import { AIProviders } from '../wailsjs/go/main/App'
import type { ProviderInfo, Settings } from './types'

// The assistants come from one list in Go (internal/ai/openai.go): the
// panel switcher, Settings and the setup hints are all built from it.
export let providers: ProviderInfo[] = []

export async function loadProviders() {
  providers = await AIProviders()
}

export function providerInfo(id: string): ProviderInfo | undefined {
  return providers.find((p) => p.id === id)
}

/** The chosen model for a provider ("" = automatic). */
export function modelOf(s: Settings, id: string): string {
  const field = providerInfo(id)?.modelField
  return field ? String((s as unknown as Record<string, unknown>)[field] ?? '') : ''
}

export function setModelOf(s: Settings, id: string, value: string) {
  const field = providerInfo(id)?.modelField
  if (field) (s as unknown as Record<string, unknown>)[field] = value
}
