import { AIProviders } from '../wailsjs/go/main/App'
import type { ai, settings } from '../wailsjs/go/models'

// The assistants come from one list in Go (internal/ai/openai.go): the
// panel switcher, Settings and the setup hints are all built from it.
export let providers: ai.ProviderInfo[] = []

export async function loadProviders() {
  providers = await AIProviders()
}

export function providerInfo(id: string): ai.ProviderInfo | undefined {
  return providers.find((p) => p.id === id)
}

/** The chosen model for a provider ("" = automatic). */
export function modelOf(s: settings.Settings, id: string): string {
  const field = providerInfo(id)?.modelField
  return field ? String((s as unknown as Record<string, unknown>)[field] ?? '') : ''
}

export function setModelOf(s: settings.Settings, id: string, value: string) {
  const field = providerInfo(id)?.modelField
  if (field) (s as unknown as Record<string, unknown>)[field] = value
}
