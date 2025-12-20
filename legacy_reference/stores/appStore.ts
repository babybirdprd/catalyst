import { create } from 'zustand';

export type ViewId = 'pipeline' | 'agents' | 'memory' | 'files' | 'factory' | 'settings';

// Core SwarmEvent types from crates/core/src/swarm/events.rs
export type SwarmEventKind =
    | 'pipeline_started'
    | 'agent_started'
    | 'agent_completed'
    | 'agent_failed'
    | 'data_passed'
    | 'critic_rejected'
    | 'pipeline_completed'
    | 'pipeline_failed';

export interface SwarmEvent {
    id: string;
    timestamp: string;
    kind: SwarmEventKind;
    agent: string;
    data?: unknown;
    unknown_id?: string;
}

export interface AgentState {
    id: string;
    status: 'idle' | 'running' | 'complete' | 'error';
    currentTask?: string;
    output: string[];
}

interface AppState {
    // Navigation
    activeView: ViewId;
    setActiveView: (view: ViewId) => void;

    // Mode
    mode: 'speed_run' | 'lab' | 'fortress';
    setMode: (mode: 'speed_run' | 'lab' | 'fortress') => void;

    // Pipeline state
    activeAgentId: string | null;
    pipelineStatus: 'idle' | 'running' | 'paused' | 'complete' | 'error';
    agents: Record<string, AgentState>;
    events: SwarmEvent[];

    // Approval
    pendingApproval: { decisionId: string; agentId: string; summary: string } | null;

    // Event handling
    handleEvent: (event: SwarmEvent) => void;
    startListening: () => void;

    // Reset
    resetPipeline: () => void;
}

const defaultAgents: Record<string, AgentState> = {
    unknowns_parser: { id: 'unknowns_parser', status: 'idle', output: [] },
    researcher: { id: 'researcher', status: 'idle', output: [] },
    architect: { id: 'architect', status: 'idle', output: [] },
    critic: { id: 'critic', status: 'idle', output: [] },
    atomizer: { id: 'atomizer', status: 'idle', output: [] },
    taskmaster: { id: 'taskmaster', status: 'idle', output: [] },
};

export const useAppStore = create<AppState>((set, get) => ({
    // Navigation
    activeView: 'pipeline',
    setActiveView: (view) => set({ activeView: view }),

    // Mode
    mode: 'lab',
    setMode: (mode) => set({ mode }),

    // Pipeline
    activeAgentId: null,
    pipelineStatus: 'idle',
    agents: { ...defaultAgents },
    events: [],

    // Approval
    pendingApproval: null,

    // Handle SSE events from core
    handleEvent: (event) => {
        // Add to event log
        set((state) => ({ events: [...state.events.slice(-99), event] }));

        switch (event.kind) {
            case 'pipeline_started':
                set({ pipelineStatus: 'running', activeAgentId: null });
                break;

            case 'agent_started':
                set((state) => ({
                    activeAgentId: event.agent,
                    agents: {
                        ...state.agents,
                        [event.agent]: {
                            ...state.agents[event.agent],
                            status: 'running',
                            currentTask: event.unknown_id ? `Processing ${event.unknown_id}` : 'Processing...',
                        },
                    },
                }));
                break;

            case 'agent_completed':
                set((state) => ({
                    agents: {
                        ...state.agents,
                        [event.agent]: {
                            ...state.agents[event.agent],
                            status: 'complete',
                            output: [...(state.agents[event.agent]?.output || []),
                            `Completed${event.unknown_id ? ` for ${event.unknown_id}` : ''}`],
                        },
                    },
                }));
                break;

            case 'agent_failed':
                set((state) => ({
                    agents: {
                        ...state.agents,
                        [event.agent]: {
                            ...state.agents[event.agent],
                            status: 'error',
                        },
                    },
                }));
                break;

            case 'critic_rejected':
                set({
                    pendingApproval: {
                        decisionId: event.id,
                        agentId: event.agent,
                        summary: `Critic rejected decision${event.unknown_id ? ` for ${event.unknown_id}` : ''}`,
                    },
                    pipelineStatus: 'paused',
                });
                break;

            case 'pipeline_completed':
                set({
                    pipelineStatus: 'complete',
                    pendingApproval: null,
                    activeAgentId: null,
                });
                break;

            case 'pipeline_failed':
                set({
                    pipelineStatus: 'error',
                    pendingApproval: null,
                    activeAgentId: null,
                });
                break;
        }
    },

    // Start SSE listener
    startListening: () => {
        const eventSource = new EventSource('/api/swarm/events');
        eventSource.onmessage = (e) => {
            try {
                const event = JSON.parse(e.data) as SwarmEvent;
                get().handleEvent(event);
            } catch (err) {
                console.error('Failed to parse SSE event:', err);
            }
        };
        eventSource.onerror = () => {
            console.log('SSE connection closed, reconnecting...');
        };
    },

    // Reset
    resetPipeline: () => set({
        activeAgentId: null,
        pipelineStatus: 'idle',
        agents: { ...defaultAgents },
        pendingApproval: null,
        events: [],
    }),
}));
