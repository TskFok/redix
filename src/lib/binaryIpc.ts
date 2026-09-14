import { invoke } from "@tauri-apps/api/core";
import { isRedisBytes, keyFromBytes, keyToBytes } from "./redisBytes";

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function inputKeys(args: Record<string, unknown>): Record<string, unknown> {
  if (!record(args.input)) return args;
  const input = { ...args.input };
  for (const name of ["key", "new_key"]) {
    if (typeof input[name] === "string") input[name] = keyToBytes(input[name]);
  }
  if (Array.isArray(input.keys)) {
    input.keys = input.keys.map(key => typeof key === "string" ? keyToBytes(key) : key);
  }
  // Import documents already use the wire format. Do not interpret their keys
  // as browser identities, or walk JSON values with properties named "key".
  return { ...args, input };
}

function outputKey(value: unknown): unknown {
  if (!record(value) || !isRedisBytes(value.key)) return value;
  return { ...value, key: keyFromBytes(value.key) };
}

function outputKeys(command: string, value: unknown): unknown {
  // Exported documents must retain the portable string/{base64} wire format.
  if (command === "export_keys") return value;
  if (command === "scan_all_keys" && Array.isArray(value)) return value.map(outputKey);
  if (!record(value)) return value;
  const result = outputKey(value) as Record<string, unknown>;
  if ((command === "scan_keys" || command === "search_keys") && Array.isArray(result.keys)) {
    return { ...result, keys: result.keys.map(outputKey) };
  }
  if (command === "search_vector_index" && Array.isArray(result.matches)) {
    return { ...result, matches: result.matches.map(outputKey) };
  }
  return result;
}

/** Convert only typed key positions. Other binary values already use RedisBytes. */
export async function invokeBinary<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const result: unknown = args === undefined ? await invoke(command) : await invoke(command, inputKeys(args));
  return outputKeys(command, result) as T;
}
