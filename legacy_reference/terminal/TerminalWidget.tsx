import { useEffect, useRef, useState } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { Maximize2, Minimize2, X } from 'lucide-react';

interface TerminalWidgetProps {
    onClose?: () => void;
}

export function TerminalWidget({ onClose }: TerminalWidgetProps) {
    const containerRef = useRef<HTMLDivElement>(null);
    const terminalRef = useRef<Terminal | null>(null);
    const wsRef = useRef<WebSocket | null>(null);
    const fitAddonRef = useRef<FitAddon | null>(null);
    const [isConnected, setIsConnected] = useState(false);
    const [isMaximized, setIsMaximized] = useState(false);

    useEffect(() => {
        if (!containerRef.current) return;

        // Create terminal
        const terminal = new Terminal({
            cursorBlink: true,
            fontSize: 13,
            fontFamily: 'Consolas, "Courier New", monospace',
            theme: {
                background: '#0a0a0f',
                foreground: '#e0e0e0',
                cursor: '#a78bfa',
                cursorAccent: '#0a0a0f',
                selectionBackground: '#7c3aed40',
                black: '#1a1a2e',
                red: '#f87171',
                green: '#4ade80',
                yellow: '#fbbf24',
                blue: '#60a5fa',
                magenta: '#c084fc',
                cyan: '#22d3ee',
                white: '#e0e0e0',
                brightBlack: '#4a4a6a',
                brightRed: '#fca5a5',
                brightGreen: '#86efac',
                brightYellow: '#fde68a',
                brightBlue: '#93c5fd',
                brightMagenta: '#d8b4fe',
                brightCyan: '#67e8f9',
                brightWhite: '#ffffff',
            },
        });

        const fitAddon = new FitAddon();
        terminal.loadAddon(fitAddon);
        terminal.open(containerRef.current);
        fitAddon.fit();

        terminalRef.current = terminal;
        fitAddonRef.current = fitAddon;

        // Connect WebSocket
        const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${protocol}//${window.location.host}/api/pty`;
        const ws = new WebSocket(wsUrl);
        wsRef.current = ws;

        ws.binaryType = 'arraybuffer';

        ws.onopen = () => {
            setIsConnected(true);
            terminal.writeln('\x1b[32m● Connected to terminal\x1b[0m\r\n');
        };

        ws.onmessage = (event) => {
            if (event.data instanceof ArrayBuffer) {
                const text = new TextDecoder().decode(event.data);
                terminal.write(text);
            } else {
                terminal.write(event.data);
            }
        };

        ws.onclose = () => {
            setIsConnected(false);
            terminal.writeln('\r\n\x1b[31m● Connection closed\x1b[0m');
        };

        ws.onerror = () => {
            terminal.writeln('\r\n\x1b[31m● Connection error\x1b[0m');
        };

        // Send terminal input to server
        terminal.onData((data) => {
            if (ws.readyState === WebSocket.OPEN) {
                ws.send(data);
            }
        });

        // Handle resize
        const handleResize = () => {
            if (fitAddonRef.current) {
                fitAddonRef.current.fit();
            }
        };

        window.addEventListener('resize', handleResize);

        // Cleanup
        return () => {
            window.removeEventListener('resize', handleResize);
            ws.close();
            terminal.dispose();
        };
    }, []);

    // Re-fit when maximized state changes
    useEffect(() => {
        if (fitAddonRef.current) {
            setTimeout(() => fitAddonRef.current?.fit(), 100);
        }
    }, [isMaximized]);

    return (
        <div
            className={`glass flex flex-col transition-all ${isMaximized ? 'fixed inset-4 z-50' : 'h-[300px]'
                }`}
        >
            {/* Header */}
            <div className="flex items-center justify-between px-4 py-2 border-b border-border bg-secondary/50">
                <div className="flex items-center gap-2">
                    <div
                        className={`w-2 h-2 rounded-full ${isConnected ? 'bg-green-500' : 'bg-red-500'
                            }`}
                    />
                    <span className="text-sm font-medium">Terminal</span>
                </div>
                <div className="flex items-center gap-1">
                    <button
                        onClick={() => setIsMaximized(!isMaximized)}
                        className="p-1.5 hover:bg-white/10 rounded transition-colors"
                        title={isMaximized ? 'Minimize' : 'Maximize'}
                    >
                        {isMaximized ? (
                            <Minimize2 className="w-4 h-4" />
                        ) : (
                            <Maximize2 className="w-4 h-4" />
                        )}
                    </button>
                    {onClose && (
                        <button
                            onClick={onClose}
                            className="p-1.5 hover:bg-red-500/20 text-red-400 rounded transition-colors"
                            title="Close"
                        >
                            <X className="w-4 h-4" />
                        </button>
                    )}
                </div>
            </div>

            {/* Terminal Container */}
            <div
                ref={containerRef}
                className="flex-1 p-2 overflow-hidden"
                style={{ minHeight: 0 }}
            />
        </div>
    );
}
