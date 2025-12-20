import { useEffect } from 'react';
import { Rocket, CheckCircle, AlertCircle, Loader2 } from 'lucide-react';
import { useReactorStore, type Feature, type PipelineStage } from '@/stores/reactorStore';

const PIPELINE_STAGES: PipelineStage[] = [
    'Idea',
    'Parsing',
    'Researching',
    'Architecting',
    'Building',
    'Testing',
    'Merging',
    'Complete',
];

const stageColors: Record<PipelineStage, string> = {
    Idea: 'bg-slate-500',
    Parsing: 'bg-blue-500',
    Researching: 'bg-cyan-500',
    Architecting: 'bg-violet-500',
    Building: 'bg-orange-500',
    Testing: 'bg-yellow-500',
    Merging: 'bg-emerald-500',
    Complete: 'bg-green-500',
    Failed: 'bg-red-500',
};

export function ReactorPipeline() {
    const { features, loadingFeatures, fetchFeatures } = useReactorStore();

    useEffect(() => {
        fetchFeatures();
        // Poll every 5 seconds for updates
        const interval = setInterval(fetchFeatures, 5000);
        return () => clearInterval(interval);
    }, [fetchFeatures]);

    return (
        <div className="flex-1 overflow-auto">
            <header className="flex items-center justify-between mb-6">
                <h1 className="text-2xl font-bold">
                    <span className="text-gradient">Reactor</span>
                </h1>
                <div className="text-sm text-muted-foreground">
                    {features.length} features in pipeline
                </div>
            </header>

            {/* Pipeline Stages Header */}
            <div className="glass p-4 mb-4">
                <div className="grid grid-cols-8 gap-2 text-xs font-semibold text-muted-foreground">
                    {PIPELINE_STAGES.map((stage) => (
                        <div key={stage} className="text-center">
                            {stage}
                        </div>
                    ))}
                </div>
            </div>

            {loadingFeatures && features.length === 0 ? (
                <div className="glass p-8 text-center">
                    <Loader2 className="w-8 h-8 animate-spin mx-auto mb-2 text-violet-400" />
                    <span className="text-muted-foreground">Loading features...</span>
                </div>
            ) : features.length === 0 ? (
                <div className="glass p-8 text-center">
                    <Rocket className="w-12 h-12 mx-auto mb-4 text-violet-400" />
                    <h3 className="font-semibold mb-2">No Features Yet</h3>
                    <p className="text-sm text-muted-foreground">
                        Add ideas in the Braindump and ignite them to start the pipeline
                    </p>
                </div>
            ) : (
                <div className="space-y-3">
                    {features.map((feature) => (
                        <FeatureRow key={feature.id} feature={feature} />
                    ))}
                </div>
            )}
        </div>
    );
}

function FeatureRow({ feature }: { feature: Feature }) {
    const currentStageIndex = PIPELINE_STAGES.indexOf(feature.stage as PipelineStage);
    const isFailed = feature.stage === 'Failed';
    const isComplete = feature.stage === 'Complete';

    return (
        <div className="glass p-4">
            {/* Feature Info */}
            <div className="flex items-center justify-between mb-3">
                <div>
                    <h3 className="font-semibold text-sm">{feature.title}</h3>
                    <span className="text-xs text-muted-foreground">{feature.id}</span>
                </div>
                <div className="flex items-center gap-2">
                    {isComplete && <CheckCircle className="w-5 h-5 text-green-400" />}
                    {isFailed && <AlertCircle className="w-5 h-5 text-red-400" />}
                    <span
                        className={`text-xs px-2 py-1 rounded font-semibold ${stageColors[feature.stage] || 'bg-slate-500'
                            } bg-opacity-20 text-white`}
                    >
                        {feature.stage}
                    </span>
                </div>
            </div>

            {/* Pipeline Progress */}
            <div className="grid grid-cols-8 gap-2">
                {PIPELINE_STAGES.map((stage, index) => {
                    const isActive = stage === feature.stage;
                    const isPast = index < currentStageIndex;
                    const isCurrent = isActive && !isComplete && !isFailed;

                    return (
                        <div
                            key={stage}
                            className={`h-2 rounded-full transition-all ${isPast || isComplete
                                ? 'bg-green-500'
                                : isCurrent
                                    ? `${stageColors[stage]} animate-pulse`
                                    : 'bg-muted'
                                } ${isFailed && isActive ? 'bg-red-500' : ''}`}
                        />
                    );
                })}
            </div>
        </div>
    );
}
