import { useAppStore } from '@/stores/appStore';
import { cn } from '@/lib/utils';

const agentList = [
    { id: 'unknowns_parser', name: 'Unknowns Parser', icon: '❓', description: 'Identifies ambiguities in user goals' },
    { id: 'researcher', name: 'Researcher', icon: '🔍', description: 'Researches options for each unknown' },
    { id: 'architect', name: 'Architect', icon: '🏗️', description: 'Makes design decisions' },
    { id: 'critic', name: 'Critic', icon: '🎯', description: 'Reviews and validates decisions' },
    { id: 'atomizer', name: 'Atomizer', icon: '⚛️', description: 'Breaks down into atomic modules' },
    { id: 'taskmaster', name: 'Taskmaster', icon: '📋', description: 'Creates mission prompts' },
];

export function AgentsView() {
    const { agents, events } = useAppStore();

    return (
        <div className="flex-1 overflow-auto">
            <header className="flex items-center justify-between mb-6">
                <h1 className="text-2xl font-bold">
                    <span className="text-gradient">Agents</span>
                </h1>
                <span className="text-sm text-muted-foreground">
                    {Object.values(agents).filter(a => a.status === 'complete').length} / 6 complete
                </span>
            </header>

            <div className="grid gap-4">
                {agentList.map((agent) => {
                    const state = agents[agent.id];

                    return (
                        <div
                            key={agent.id}
                            className={cn(
                                "glass p-4 flex items-start gap-4 transition-all",
                                state?.status === 'running' && "border-violet-500 shadow-[0_0_20px_rgba(139,92,246,0.3)]",
                                state?.status === 'complete' && "border-green-500/50",
                                state?.status === 'error' && "border-red-500/50"
                            )}
                        >
                            <div className={cn(
                                "text-3xl w-14 h-14 flex items-center justify-center rounded-xl",
                                state?.status === 'running' && "bg-violet-500/20 animate-pulse",
                                state?.status === 'complete' && "bg-green-500/20",
                                state?.status === 'error' && "bg-red-500/20",
                                state?.status === 'idle' && "bg-muted"
                            )}>
                                {agent.icon}
                            </div>

                            <div className="flex-1">
                                <div className="flex items-center gap-2">
                                    <h3 className="font-semibold">{agent.name}</h3>
                                    <span className={cn(
                                        "text-[10px] px-2 py-0.5 rounded-full font-semibold uppercase",
                                        state?.status === 'running' && "bg-violet-500/20 text-violet-400",
                                        state?.status === 'complete' && "bg-green-500/20 text-green-400",
                                        state?.status === 'error' && "bg-red-500/20 text-red-400",
                                        state?.status === 'idle' && "bg-muted text-muted-foreground"
                                    )}>
                                        {state?.status || 'idle'}
                                    </span>
                                </div>
                                <p className="text-sm text-muted-foreground mt-1">{agent.description}</p>

                                {state?.output.length > 0 && (
                                    <div className="mt-2 text-xs text-cyan-400 font-mono">
                                        {state.output[state.output.length - 1]}
                                    </div>
                                )}
                            </div>
                        </div>
                    );
                })}
            </div>

            {/* Event Log */}
            <section className="mt-6">
                <h2 className="text-sm font-semibold text-muted-foreground mb-3">Event Log ({events.length})</h2>
                <div className="glass p-3 max-h-48 overflow-y-auto space-y-1">
                    {events.length === 0 ? (
                        <div className="text-xs text-muted-foreground text-center py-2">No events yet</div>
                    ) : (
                        events.slice().reverse().map((event) => (
                            <div key={event.id} className="text-xs font-mono flex gap-2">
                                <span className="text-muted-foreground">{new Date(event.timestamp).toLocaleTimeString()}</span>
                                <span className={cn(
                                    event.kind.includes('completed') && "text-green-400",
                                    event.kind.includes('started') && "text-violet-400",
                                    event.kind.includes('failed') || event.kind.includes('rejected') && "text-red-400"
                                )}>
                                    {event.kind}
                                </span>
                                <span className="text-muted-foreground">→ {event.agent}</span>
                            </div>
                        ))
                    )}
                </div>
            </section>
        </div>
    );
}
