declare module "lz4js" {
  export function decompressBlock(source: Uint8Array, destination: Uint8Array, sourceStart: number, sourceLength: number, destinationStart: number): number;
}
declare module "json-bigint" {
  export default function JSONBigInt(options?: { useNativeBigInt?: boolean }): { stringify(value: unknown, replacer?: unknown, space?: number): string };
}
