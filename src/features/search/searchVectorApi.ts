import { invokeBinary as invoke } from "../../lib/binaryIpc";
import type { SearchVectorQueryInput, SearchVectorQueryResult } from "../../lib/types";
export const searchVectorIndex = (input: SearchVectorQueryInput) => invoke<SearchVectorQueryResult>("search_vector_index", { input });
