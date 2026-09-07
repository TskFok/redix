import { decodeStructuredValue } from "./structuredValueCodec";
import type { StructuredValueFormat } from "./structuredValueCodec";

self.onmessage = (event: MessageEvent<{ bytes: Uint8Array; format: StructuredValueFormat }>) => {
  try { self.postMessage({ ok: true, text: decodeStructuredValue(event.data.bytes, event.data.format) }); }
  catch { self.postMessage({ ok: false }); }
};
