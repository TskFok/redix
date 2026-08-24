import type { IpcError, QueryLibraryItem } from "../../lib/types";

export interface QueryLibraryPageState {
  items: QueryLibraryItem[];
  query: string;
  loading: boolean;
  saving: boolean;
  error: IpcError | null;
}

export const initialQueryLibraryPageState: QueryLibraryPageState = {
  items: [],
  query: "",
  loading: false,
  saving: false,
  error: null,
};

const queryLibraryErrorMessages: Record<string, string> = {
  INVALID_CONNECTION: "查询无法保存，请检查名称、命令和标签。",
  PERSISTENCE_FAILED: "本地查询保存失败，请稍后重试。",
  IPC_ERROR: "桌面端调用失败，请稍后重试。",
};

export function filterQueryLibraryItems(
  items: QueryLibraryItem[],
  query: string,
): QueryLibraryItem[] {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  if (!normalizedQuery) {
    return items;
  }

  return items.filter((item) =>
    [item.name, item.command, ...item.tags].some((value) =>
      value.toLocaleLowerCase().includes(normalizedQuery),
    ),
  );
}

export function normalizeQueryLibraryError(error: unknown): IpcError {
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    typeof error.code === "string" &&
    queryLibraryErrorMessages[error.code]
  ) {
    return {
      code: error.code,
      message: queryLibraryErrorMessages[error.code],
    };
  }

  return {
    code: "PERSISTENCE_FAILED",
    message: queryLibraryErrorMessages.PERSISTENCE_FAILED,
  };
}

export function normalizeTags(tags: string): string[] {
  return tags
    .split(",")
    .map((tag) => tag.trim())
    .filter((tag) => tag.length > 0);
}
