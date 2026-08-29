import { Channel, invoke } from '@tauri-apps/api/core'

export interface CosyvoiceConfig {
  api_key_configured: boolean
  models: string[]
}

export interface CosyVoiceView {
  voice_id: string
  name: string
  model: string
  status: string | null
}

export interface CosyVoiceRecord {
  voice_id: string
  name: string
  model: string
  created_at: string | null
}

export interface CosyvoiceProgress {
  phase: string
}

export function getConfig(): Promise<CosyvoiceConfig> {
  return invoke<CosyvoiceConfig>('cosyvoice_get_config')
}

export function saveApiKey(apiKey: string): Promise<void> {
  return invoke<void>('cosyvoice_save_api_key', { apiKey })
}

export function addModel(model: string): Promise<void> {
  return invoke<void>('cosyvoice_add_model', { model })
}

export function removeModel(model: string): Promise<void> {
  return invoke<void>('cosyvoice_remove_model', { model })
}

export async function createVoice(
  name: string,
  model: string,
  filePath: string,
  onProgress: (phase: string) => void,
): Promise<CosyVoiceRecord> {
  const channel = new Channel<CosyvoiceProgress>()
  channel.onmessage = (event) => onProgress(event.phase)
  return invoke<CosyVoiceRecord>('cosyvoice_create_voice', {
    name,
    model,
    filePath,
    channel,
  })
}

export function createVoiceFromUrl(
  name: string,
  model: string,
  url: string,
): Promise<CosyVoiceRecord> {
  return invoke<CosyVoiceRecord>('cosyvoice_create_voice_from_url', { name, model, url })
}

export function listVoices(): Promise<CosyVoiceView[]> {
  return invoke<CosyVoiceView[]>('cosyvoice_list_voices')
}

export function deleteVoice(voiceId: string): Promise<void> {
  return invoke<void>('cosyvoice_delete_voice', { voiceId })
}

export function synthesizePreview(
  model: string,
  voiceId: string,
  text: string,
): Promise<Uint8Array> {
  return invoke<Uint8Array>('cosyvoice_synthesize_preview', { model, voiceId, text })
}
