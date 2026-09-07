import { invoke } from "@tauri-apps/api/core";
import type { SearchVectorQueryInput, SearchVectorQueryResult } from "../../lib/types";
export const searchVectorIndex = (input: SearchVectorQueryInput) => invoke<SearchVectorQueryResult>("search_vector_index", { input });
