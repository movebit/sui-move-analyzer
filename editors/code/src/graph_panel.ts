// Copyright (c) The BitsLab.MoveBit Contributors
// SPDX-License-Identifier: Apache-2.0

import { Context } from './context';
import * as vscode from 'vscode';

interface GraphData {
    nodes: Array<{
        id: string;
        label: string;
        module?: string;
        address?: string;
        signature?: string;
    }>;
    edges: Array<{
        from: string;
        to: string;
        label?: string;
        arrows?: string;
    }>;
}

export class GraphPanel {
    private panel: vscode.WebviewPanel | undefined;
    private disposables: vscode.Disposable[] = [];

    constructor(private context: Context, private extensionUri: vscode.Uri) {}

    public async showStructDependencyGraph() {
        await this.showGraph('Struct Dependency Graph', 'struct_dependency');
    }

    public async showCallFlowGraph() {
        await this.showGraph('Function Call Flow Graph', 'call_flow');
    }

    private async showGraph(title: string, graphType: string) {
        const editor = vscode.window.activeTextEditor;
        if (!editor) {
            vscode.window.showErrorMessage('No active editor found');
            return;
        }

        const document = editor.document;
        if (document.languageId !== 'move') {
            vscode.window.showErrorMessage('Current file is not a Move file');
            return;
        }

        // Create or reveal the webview panel
        if (this.panel) {
            this.panel.reveal(vscode.ViewColumn.One);
        } else {
            this.panel = vscode.window.createWebviewPanel(
                `${graphType}Graph`,
                title,
                vscode.ViewColumn.One,
                {
                    enableScripts: true,
                    retainContextWhenHidden: true
                }
            );

            this.panel.webview.html = this.getHtmlForWebview(this.panel.webview);

            // Handle panel disposal
            this.panel.onDidDispose(() => {
                this.panel = undefined;
                this.disposables.forEach(d => d.dispose());
                this.disposables = [];
            }, null, this.disposables);
        }

        // Request graph data from the language server
        const client = this.context.getClient();
        if (!client) {
            vscode.window.showErrorMessage('No language server client available');
            return;
        }

        try {
            const graphData = await client.sendRequest<any>(
                `move/${graphType}/graph`,
                {
                    textDocument: {
                        uri: document.uri.toString()
                    },
                    graphType: graphType
                }
            );

            if (graphData && graphData.graph_data) {
                const parsedData: GraphData = JSON.parse(graphData.graph_data);
                this.panel.webview.postMessage({
                    command: 'renderGraph',
                    data: parsedData,
                    graphType: graphType
                });
            } else {
                vscode.window.showErrorMessage(`No ${graphType} graph data received`);
            }
        } catch (error) {
            vscode.window.showErrorMessage(`Failed to get ${graphType} graph: ${error}`);
        }
    }

    private getHtmlForWebview(webview: vscode.Webview): string {
        // Get the URIs for the scripts and styles
        const visNetworkJsUri = webview.asWebviewUri(
            vscode.Uri.joinPath(this.extensionUri, 'media', 'vis-network.min.js')
        );
        const visNetworkCssUri = webview.asWebviewUri(
            vscode.Uri.joinPath(this.extensionUri, 'media', 'vis-network.min.css')
        );

        return `
            <!DOCTYPE html>
            <html lang="en">
            <head>
                <meta charset="UTF-8">
                <meta name="viewport" content="width=device-width, initial-scale=1.0">
                <title>Move Graph Visualization</title>
                <link href="${visNetworkCssUri}" rel="stylesheet" />
                <style>
                    body {
                        margin: 0;
                        padding: 0;
                        height: 100vh;
                        overflow: hidden;
                        background-color: #fff;
                    }
                    #graph-container {
                        width: 100%;
                        height: 100%;
                        border: 1px solid #ccc;
                    }
                    .controls {
                        position: absolute;
                        top: 10px;
                        right: 10px;
                        z-index: 1000;
                        background: white;
                        padding: 10px;
                        border-radius: 4px;
                        box-shadow: 0 2px 10px rgba(0,0,0,0.1);
                    }
                </style>
            </head>
            <body>
                <div id="graph-container"></div>
                <div class="controls">
                    <button onclick="fitToScreen()">Fit to Screen</button>
                    <button onclick="centerView()">Center</button>
                </div>
                <script src="${visNetworkJsUri}"></script>
                <script>
                    const graphContainer = document.getElementById('graph-container');
                    let network = null;

                    window.addEventListener('message', event => {
                        const message = event.data;
                        switch (message.command) {
                            case 'renderGraph':
                                renderGraph(message.data, message.graphType);
                                break;
                        }
                    });

                    function renderGraph(data, graphType) {
                        if (network) {
                            network.destroy();
                        }

                        // Prepare the data for Vis Network
                        const nodes = new vis.DataSet(data.nodes.map(node => ({
                            id: node.id,
                            label: node.label,
                            title: generateNodeTitle(node, graphType),
                            group: graphType === 'struct_dependency' ? 'structs' : 'functions'
                        })));

                        const edges = new vis.DataSet(data.edges.map(edge => ({
                            from: edge.from,
                            to: edge.to,
                            label: edge.label || '',
                            arrows: edge.arrows || 'to'
                        })));

                        const container = document.getElementById('graph-container');
                        const graphData = {
                            nodes: nodes,
                            edges: edges
                        };

                        const options = {
                            nodes: {
                                shape: 'box',
                                font: {
                                    size: 14,
                                    face: 'Arial'
                                },
                                color: {
                                    background: graphType === 'struct_dependency' ? '#e0f7fa' : '#fff3e0',
                                    border: graphType === 'struct_dependency' ? '#00838f' : '#f57c00',
                                    highlight: {
                                        background: graphType === 'struct_dependency' ? '#b2ebf2' : '#ffe0b2',
                                        border: graphType === 'struct_dependency' ? '#006064' : '#ef6c00'
                                    }
                                },
                                borderWidth: 2
                            },
                            edges: {
                                width: 2,
                                color: {
                                    color: graphType === 'struct_dependency' ? '#00838f' : '#f57c00',
                                    highlight: graphType === 'struct_dependency' ? '#006064' : '#ef6c00'
                                },
                                smooth: {
                                    type: 'curvedCW',
                                    roundness: 0.2
                                },
                                font: {
                                    size: 12,
                                    align: 'middle'
                                }
                            },
                            physics: {
                                enabled: true,
                                stabilization: { iterations: 100 }
                            },
                            interaction: {
                                tooltipDelay: 200,
                                hideEdgesOnDrag: false
                            }
                        };

                        network = new vis.Network(container, graphData, options);
                    }

                    function generateNodeTitle(node, graphType) {
                        let title = '<div style="padding: 5px;">';
                        title += '<h3>' + node.label + '</h3>';
                        
                        if (node.module) {
                            title += '<p><strong>Module:</strong> ' + node.module + '</p>';
                        }
                        
                        if (node.address) {
                            title += '<p><strong>Address:</strong> ' + node.address + '</p>';
                        }
                        
                        if (node.signature) {
                            title += '<p><strong>Signature:</strong> ' + node.signature + '</p>';
                        }
                        
                        title += '<p><strong>Type:</strong> ' + (graphType === 'struct_dependency' ? 'Struct' : 'Function') + '</p>';
                        title += '</div>';
                        
                        return title;
                    }

                    function fitToScreen() {
                        if (network) {
                            network.fit();
                        }
                    }

                    function centerView() {
                        if (network) {
                            network.moveTo({position: {x: 0, y: 0}});
                        }
                    }
                </script>
            </body>
            </html>
        `;
    }
}