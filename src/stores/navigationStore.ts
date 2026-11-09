import { create } from "zustand";
import type { PageId } from "@/types";

interface NavigationStore {
  page: PageId;
  navigate: (page: PageId) => void;
}

export const useNavigation = create<NavigationStore>((set) => ({
  page: "dashboard",
  navigate: (page) => set({ page }),
}));
