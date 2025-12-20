import { create } from 'zustand';

// === Braindump Types ===
export interface Idea {
    id: string;
    content: string;
    created_at: string;
}

export interface ContextFile {
    path: string;
    size: number;
    extension: string | null;
    ingested_at: string;
}

// === Reactor Types ===
export type PipelineStage =
    | 'Idea'
    | 'Parsing'
    | 'Researching'
    | 'Architecting'
    | 'Building'
    | 'Testing'
    | 'Merging'
    | 'Complete'
    | 'Failed';

export interface Feature {
    id: string;
    title: string;
    stage: PipelineStage;
    description: string | null;
    created_at: string;
}

// === Store State ===
interface ReactorState {
    // Braindump
    ideas: Idea[];
    files: ContextFile[];
    loadingBraindump: boolean;

    // Features
    features: Feature[];
    loadingFeatures: boolean;

    // Actions
    fetchBraindump: () => Promise<void>;
    createIdea: (content: string) => Promise<boolean>;
    fetchFeatures: () => Promise<void>;
    igniteIdea: (ideaId: string) => Promise<string | null>;
}

export const useReactorStore = create<ReactorState>((set, get) => ({
    // Initial state
    ideas: [],
    files: [],
    loadingBraindump: false,
    features: [],
    loadingFeatures: false,

    // Fetch braindump (ideas + files)
    fetchBraindump: async () => {
        set({ loadingBraindump: true });
        try {
            const res = await fetch('/api/braindump');
            const data = await res.json();
            set({
                ideas: data.ideas || [],
                files: data.files || [],
            });
        } catch (err) {
            console.error('Failed to fetch braindump:', err);
        } finally {
            set({ loadingBraindump: false });
        }
    },

    // Create a new idea
    createIdea: async (content: string) => {
        try {
            const res = await fetch('/api/braindump/ideas', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ content }),
            });
            const data = await res.json();
            if (data.success) {
                // Refresh braindump
                get().fetchBraindump();
                return true;
            }
            return false;
        } catch (err) {
            console.error('Failed to create idea:', err);
            return false;
        }
    },

    // Fetch all features
    fetchFeatures: async () => {
        set({ loadingFeatures: true });
        try {
            const res = await fetch('/api/reactor/features');
            const data = await res.json();
            set({ features: data || [] });
        } catch (err) {
            console.error('Failed to fetch features:', err);
        } finally {
            set({ loadingFeatures: false });
        }
    },

    // Promote idea to feature ("ignite")
    igniteIdea: async (ideaId: string) => {
        try {
            const res = await fetch('/api/reactor/ignite', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ idea_id: ideaId }),
            });
            const data = await res.json();
            if (data.success && data.feature_id) {
                // Refresh both
                get().fetchBraindump();
                get().fetchFeatures();
                return data.feature_id;
            }
            return null;
        } catch (err) {
            console.error('Failed to ignite idea:', err);
            return null;
        }
    },
}));
