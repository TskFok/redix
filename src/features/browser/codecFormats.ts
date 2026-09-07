export type BasicValueFormat = "utf8" | "json" | "ascii" | "hex" | "binary" | "base64";
export type ValueFormat = BasicValueFormat | "msgpack" | "protobuf" | "php";
export const isStructuredFormat = (format: ValueFormat) => ["msgpack", "protobuf", "php"].includes(format);
