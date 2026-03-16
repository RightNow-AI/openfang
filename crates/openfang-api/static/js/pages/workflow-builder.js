// OpenFang Visual Workflow Builder — Drag-and-drop workflow designer
'use strict';

function workflowBuilder() {
  return {
    // -- Canvas state --
    nodes: [],
    connections: [],
    selectedNode: null,
    selectedConnection: null,
    dragging: null,
    dragOffset: { x: 0, y: 0 },
    connecting: null, // { fromId, fromPort }
    connectPreview: null, // { x, y } mouse position during connect drag
    canvasOffset: { x: 0, y: 0 },
    canvasDragging: false,
    canvasDragStart: { x: 0, y: 0 },
    zoom: 1,
    nextId: 1,
    workflowName: '',
    workflowDescription: '',
    showSaveModal: false,
    showNodeEditor: false,
    showTomlPreview: false,
    tomlOutput: '',
    agents: [],
    _canvasEl: null,

    // Node types with their configs
    nodeTypes: [
      { type: 'agent', label: 'Agent Step', color: '#6366f1', icon: 'A', ports: { in: 1, out: 1 } },
      { type: 'parallel', label: 'Parallel Fan-out', color: '#f59e0b', icon: 'P', ports: { in: 1, out: 3 } },
      { type: 'condition', label: 'Condition', color: '#10b981', icon: '?', ports: { in: 1, out: 2 } },
      { type: 'loop', label: 'Loop', color: '#ef4444', icon: 'L', ports: { in: 1, out: 1 } },
      { type: 'collect', label: 'Collect', color: '#8b5cf6', icon: 'C', ports: { in: 3, out: 1 } },
      { type: 'start', label: 'Start', color: '#22c55e', icon: 'S', ports: { in: 0, out: 1 } },
      { type: 'end', label: 'End', color: '#ef4444', icon: 'E', ports: { in: 1, out: 0 } }
    ],

    _renderScheduled: false,
    _lastClickNodeId: null,
    _lastClickTime: 0,
    _didDrag: false,
    _didConnect: false,
    _didPan: false,

    async init() {
      var self = this;
      // Load agents for the agent step dropdown
      try {
        var list = await OpenFangAPI.get('/api/agents');
        self.agents = Array.isArray(list) ? list : [];
      } catch(_) {
        self.agents = [];
      }
      // Add default start node
      self.addNode('start', 60, 200);
    },

    // ── SVG Manual Rendering ────────────────────────────
    // Alpine.js x-for inside <svg> breaks because document.importNode
    // doesn't handle SVG namespace correctly. We render nodes/connections
    // manually via createElementNS and schedule re-renders reactively.

    scheduleRender: function() {
      if (this._renderScheduled) return;
      this._renderScheduled = true;
      var self = this;
      requestAnimationFrame(function() {
        self._renderScheduled = false;
        self.renderCanvas();
      });
    },

    renderCanvas: function() {
      var container = document.getElementById('wf-render-group');
      if (!container) return;
      var SVG_NS = 'http://www.w3.org/2000/svg';
      var self = this;

      // Clear previous rendered content
      while (container.firstChild) container.removeChild(container.firstChild);

      // ── Connections ──
      for (var ci = 0; ci < this.connections.length; ci++) {
        var conn = this.connections[ci];
        var d = this.getConnectionPath(conn);
        if (!d) continue;
        var path = document.createElementNS(SVG_NS, 'path');
        path.setAttribute('d', d);
        path.setAttribute('fill', 'none');
        path.setAttribute('stroke', (this.selectedConnection && this.selectedConnection.id === conn.id) ? 'var(--accent)' : 'var(--text-dim)');
        path.setAttribute('stroke-width', (this.selectedConnection && this.selectedConnection.id === conn.id) ? '3' : '2');
        path.style.cursor = 'pointer';
        (function(c) {
          path.addEventListener('click', function(e) { e.stopPropagation(); self.selectedConnection = c; self.scheduleRender(); });
        })(conn);
        container.appendChild(path);
      }

      // ── Connection preview ──
      if (this.connecting && this.connectPreview) {
        var pd = this.getPreviewPath();
        if (pd) {
          var preview = document.createElementNS(SVG_NS, 'path');
          preview.setAttribute('d', pd);
          preview.setAttribute('fill', 'none');
          preview.setAttribute('stroke', 'var(--accent)');
          preview.setAttribute('stroke-width', '2');
          preview.setAttribute('stroke-dasharray', '6,3');
          container.appendChild(preview);
        }
      }

      // ── Nodes ──
      for (var ni = 0; ni < this.nodes.length; ni++) {
        var node = this.nodes[ni];
        var g = document.createElementNS(SVG_NS, 'g');
        g.classList.add('wf-node');
        g.setAttribute('transform', 'translate(' + node.x + ',' + node.y + ')');
        (function(n) {
          g.addEventListener('mousedown', function(e) { self.onNodeMouseDown(n, e); });
          g.addEventListener('dblclick', function() { self.editNode(n); });
        })(node);

        // Node body rect
        var rect = document.createElementNS(SVG_NS, 'rect');
        rect.setAttribute('x', '0'); rect.setAttribute('y', '0');
        rect.setAttribute('width', node.width); rect.setAttribute('height', node.height);
        rect.setAttribute('rx', '8'); rect.setAttribute('ry', '8');
        rect.setAttribute('fill', (self.selectedNode && self.selectedNode.id === node.id) ? 'var(--card-bg)' : 'var(--bg-secondary)');
        rect.setAttribute('stroke', (self.selectedNode && self.selectedNode.id === node.id) ? node.color : 'var(--border)');
        rect.setAttribute('stroke-width', '2');
        rect.style.cursor = 'grab';
        g.appendChild(rect);

        // Color accent bar
        var bar = document.createElementNS(SVG_NS, 'rect');
        bar.setAttribute('x', '0'); bar.setAttribute('y', '0');
        bar.setAttribute('width', '6'); bar.setAttribute('height', node.height);
        bar.setAttribute('rx', '3'); bar.setAttribute('ry', '0');
        bar.setAttribute('fill', node.color);
        g.appendChild(bar);

        // Icon circle + text
        var circle = document.createElementNS(SVG_NS, 'circle');
        circle.setAttribute('cx', '28'); circle.setAttribute('cy', node.height / 2);
        circle.setAttribute('r', '14'); circle.setAttribute('fill', node.color);
        circle.setAttribute('opacity', '0.15');
        g.appendChild(circle);

        var iconText = document.createElementNS(SVG_NS, 'text');
        iconText.setAttribute('x', '28'); iconText.setAttribute('y', node.height / 2 + 4);
        iconText.setAttribute('text-anchor', 'middle'); iconText.setAttribute('fill', node.color);
        iconText.setAttribute('style', 'font-size:12px;font-weight:700;pointer-events:none');
        iconText.textContent = node.icon;
        g.appendChild(iconText);

        // Label
        var label = document.createElementNS(SVG_NS, 'text');
        label.setAttribute('x', '50'); label.setAttribute('y', node.height / 2 - 4);
        label.setAttribute('fill', 'var(--text)');
        label.setAttribute('style', 'font-size:12px;font-weight:600;pointer-events:none');
        label.textContent = node.label;
        g.appendChild(label);

        // Sub-label
        var subLabel = document.createElementNS(SVG_NS, 'text');
        subLabel.setAttribute('x', '50'); subLabel.setAttribute('y', node.height / 2 + 12);
        subLabel.setAttribute('fill', 'var(--text-dim)');
        subLabel.setAttribute('style', 'font-size:10px;pointer-events:none');
        if (node.type === 'agent') subLabel.textContent = node.config.agent_name || 'No agent';
        else if (node.type === 'condition') subLabel.textContent = node.config.expression || 'No condition';
        else if (node.type === 'loop') subLabel.textContent = 'max ' + (node.config.max_iterations || 5) + ' iters';
        else if (node.type === 'parallel') subLabel.textContent = (node.config.fan_count || 3) + ' branches';
        else if (node.type === 'collect') subLabel.textContent = node.config.strategy || 'all';
        g.appendChild(subLabel);

        // Input ports
        for (var pi = 0; pi < node.ports.in; pi++) {
          var inp = document.createElementNS(SVG_NS, 'circle');
          inp.classList.add('wf-port', 'wf-port-in');
          inp.setAttribute('cx', node.width / (node.ports.in + 1) * (pi + 1));
          inp.setAttribute('cy', '0'); inp.setAttribute('r', '6');
          inp.setAttribute('fill', 'var(--bg-secondary)');
          inp.setAttribute('stroke', 'var(--text-dim)'); inp.setAttribute('stroke-width', '2');
          (function(nid, idx) {
            inp.addEventListener('mouseup', function(e) { e.stopPropagation(); self.endConnect(nid, idx, e); });
          })(node.id, pi);
          g.appendChild(inp);
        }

        // Output ports
        for (var po = 0; po < node.ports.out; po++) {
          var outp = document.createElementNS(SVG_NS, 'circle');
          outp.classList.add('wf-port', 'wf-port-out');
          outp.setAttribute('cx', node.width / (node.ports.out + 1) * (po + 1));
          outp.setAttribute('cy', node.height); outp.setAttribute('r', '6');
          outp.setAttribute('fill', 'var(--bg-secondary)');
          outp.setAttribute('stroke', node.color); outp.setAttribute('stroke-width', '2');
          (function(nid, idx) {
            outp.addEventListener('mousedown', function(e) { e.stopPropagation(); self.startConnect(nid, idx, e); });
          })(node.id, po);
          g.appendChild(outp);
        }

        container.appendChild(g);
      }
    },

    // ── Node Management ──────────────────────────────────

    addNode: function(type, x, y) {
      var def = null;
      for (var i = 0; i < this.nodeTypes.length; i++) {
        if (this.nodeTypes[i].type === type) { def = this.nodeTypes[i]; break; }
      }
      if (!def) return;
      var node = {
        id: 'node-' + this.nextId++,
        type: type,
        label: def.label,
        color: def.color,
        icon: def.icon,
        x: x || 200,
        y: y || 200,
        width: 180,
        height: 70,
        ports: { in: def.ports.in, out: def.ports.out },
        config: {}
      };
      if (type === 'agent') {
        node.config = { agent_name: '', prompt: '{{input}}', model: '' };
      } else if (type === 'condition') {
        node.config = { expression: '', true_label: 'Yes', false_label: 'No' };
      } else if (type === 'loop') {
        node.config = { max_iterations: 5, until: '' };
      } else if (type === 'parallel') {
        node.config = { fan_count: 3 };
      } else if (type === 'collect') {
        node.config = { strategy: 'all' };
      }
      this.nodes.push(node);
      this.scheduleRender();
      return node;
    },

    deleteNode: function(nodeId) {
      this.connections = this.connections.filter(function(c) {
        return c.from !== nodeId && c.to !== nodeId;
      });
      this.nodes = this.nodes.filter(function(n) { return n.id !== nodeId; });
      if (this.selectedNode && this.selectedNode.id === nodeId) {
        this.selectedNode = null;
        this.showNodeEditor = false;
      }
      this.scheduleRender();
    },

    duplicateNode: function(node) {
      var newNode = this.addNode(node.type, node.x + 30, node.y + 30);
      if (newNode) {
        newNode.config = JSON.parse(JSON.stringify(node.config));
        newNode.label = node.label + ' copy';
      }
    },

    getNode: function(id) {
      for (var i = 0; i < this.nodes.length; i++) {
        if (this.nodes[i].id === id) return this.nodes[i];
      }
      return null;
    },

    // ── Port Positions ───────────────────────────────────

    getInputPortPos: function(node, portIndex) {
      var total = node.ports.in;
      var spacing = node.width / (total + 1);
      return { x: node.x + spacing * (portIndex + 1), y: node.y };
    },

    getOutputPortPos: function(node, portIndex) {
      var total = node.ports.out;
      var spacing = node.width / (total + 1);
      return { x: node.x + spacing * (portIndex + 1), y: node.y + node.height };
    },

    // ── Connection Management ────────────────────────────

    startConnect: function(nodeId, portIndex, e) {
      e.stopPropagation();
      this.connecting = { fromId: nodeId, fromPort: portIndex };
      var node = this.getNode(nodeId);
      var pos = this.getOutputPortPos(node, portIndex);
      this.connectPreview = { x: pos.x, y: pos.y };
    },

    endConnect: function(nodeId, portIndex, e) {
      e.stopPropagation();
      if (!this.connecting) return;
      if (this.connecting.fromId === nodeId) {
        this.connecting = null;
        this.connectPreview = null;
        return;
      }
      // Check for duplicate
      var fromId = this.connecting.fromId;
      var fromPort = this.connecting.fromPort;
      var dup = false;
      for (var i = 0; i < this.connections.length; i++) {
        var c = this.connections[i];
        if (c.from === fromId && c.fromPort === fromPort && c.to === nodeId && c.toPort === portIndex) {
          dup = true;
          break;
        }
      }
      if (!dup) {
        this.connections.push({
          id: 'conn-' + this.nextId++,
          from: fromId,
          fromPort: fromPort,
          to: nodeId,
          toPort: portIndex
        });
      }
      this.connecting = null;
      this.connectPreview = null;
      this.scheduleRender();
    },

    deleteConnection: function(connId) {
      this.connections = this.connections.filter(function(c) { return c.id !== connId; });
      this.selectedConnection = null;
      this.scheduleRender();
    },

    // ── Drag Handling ────────────────────────────────────

    onNodeMouseDown: function(node, e) {
      e.stopPropagation();
      // Detect double-click manually — the native dblclick event never fires
      // because scheduleRender() destroys and recreates all SVG elements between
      // the first and second click, so the browser loses the DOM target for dblclick.
      var now = Date.now();
      if (this._lastClickNodeId === node.id && (now - this._lastClickTime) < 350) {
        // Double-click detected — open editor instead of starting drag
        this._lastClickNodeId = null;
        this._lastClickTime = 0;
        this.editNode(node);
        return;
      }
      this._lastClickNodeId = node.id;
      this._lastClickTime = now;

      this.selectedNode = node;
      this.selectedConnection = null;
      this._didDrag = false;
      this.dragging = node.id;
      var rect = this._getCanvasRect();
      this.dragOffset = {
        x: (e.clientX - rect.left) / this.zoom - this.canvasOffset.x - node.x,
        y: (e.clientY - rect.top) / this.zoom - this.canvasOffset.y - node.y
      };
    },

    onCanvasMouseDown: function(e) {
      if (e.target.closest('.wf-node') || e.target.closest('.wf-port')) return;
      this.selectedNode = null;
      this.selectedConnection = null;
      this.showNodeEditor = false;
      // Start canvas pan
      this._didPan = false;
      this.canvasDragging = true;
      this.canvasDragStart = { x: e.clientX - this.canvasOffset.x * this.zoom, y: e.clientY - this.canvasOffset.y * this.zoom };
    },

    onCanvasMouseMove: function(e) {
      var rect = this._getCanvasRect();
      if (this.dragging) {
        this._didDrag = true;
        var node = this.getNode(this.dragging);
        if (node) {
          node.x = Math.max(0, (e.clientX - rect.left) / this.zoom - this.canvasOffset.x - this.dragOffset.x);
          node.y = Math.max(0, (e.clientY - rect.top) / this.zoom - this.canvasOffset.y - this.dragOffset.y);
        }
        this.scheduleRender();
      } else if (this.connecting) {
        this._didConnect = true;
        this.connectPreview = {
          x: (e.clientX - rect.left) / this.zoom - this.canvasOffset.x,
          y: (e.clientY - rect.top) / this.zoom - this.canvasOffset.y
        };
        this.scheduleRender();
      } else if (this.canvasDragging) {
        this._didPan = true;
        this.canvasOffset = {
          x: (e.clientX - this.canvasDragStart.x) / this.zoom,
          y: (e.clientY - this.canvasDragStart.y) / this.zoom
        };
      }
    },

    onCanvasMouseUp: function() {
      // Only re-render if something actually moved. Rendering on every mouseup
      // destroys SVG elements between clicks, which prevents dblclick detection.
      var needsRender = this._didDrag || this._didConnect || this._didPan;
      this.dragging = null;
      this.connecting = null;
      this.connectPreview = null;
      this.canvasDragging = false;
      this._didDrag = false;
      this._didConnect = false;
      this._didPan = false;
      if (needsRender) {
        this.scheduleRender();
      }
    },

    onCanvasWheel: function(e) {
      e.preventDefault();
      var delta = e.deltaY > 0 ? -0.05 : 0.05;
      this.zoom = Math.max(0.3, Math.min(2, this.zoom + delta));
    },

    _getCanvasRect: function() {
      if (!this._canvasEl) {
        this._canvasEl = document.getElementById('wf-canvas');
      }
      return this._canvasEl ? this._canvasEl.getBoundingClientRect() : { left: 0, top: 0 };
    },

    // ── Connection Path ──────────────────────────────────

    getConnectionPath: function(conn) {
      var fromNode = this.getNode(conn.from);
      var toNode = this.getNode(conn.to);
      if (!fromNode || !toNode) return '';
      var from = this.getOutputPortPos(fromNode, conn.fromPort);
      var to = this.getInputPortPos(toNode, conn.toPort);
      var dy = Math.abs(to.y - from.y);
      var cp = Math.max(40, dy * 0.5);
      return 'M ' + from.x + ' ' + from.y + ' C ' + from.x + ' ' + (from.y + cp) + ' ' + to.x + ' ' + (to.y - cp) + ' ' + to.x + ' ' + to.y;
    },

    getPreviewPath: function() {
      if (!this.connecting || !this.connectPreview) return '';
      var fromNode = this.getNode(this.connecting.fromId);
      if (!fromNode) return '';
      var from = this.getOutputPortPos(fromNode, this.connecting.fromPort);
      var to = this.connectPreview;
      var dy = Math.abs(to.y - from.y);
      var cp = Math.max(40, dy * 0.5);
      return 'M ' + from.x + ' ' + from.y + ' C ' + from.x + ' ' + (from.y + cp) + ' ' + to.x + ' ' + (to.y - cp) + ' ' + to.x + ' ' + to.y;
    },

    // ── Node editor ──────────────────────────────────────

    editNode: function(node) {
      this.selectedNode = node;
      this.showNodeEditor = true;
      this.scheduleRender();
    },

    // Called from editor panel inputs to reflect changes on the canvas SVG
    applyNodeEdit: function() {
      this.scheduleRender();
    },

    // ── TOML Generation ──────────────────────────────────

    generateToml: function() {
      var self = this;
      var lines = [];
      lines.push('[workflow]');
      lines.push('name = "' + (this.workflowName || 'untitled') + '"');
      lines.push('description = "' + (this.workflowDescription || '') + '"');
      lines.push('');

      // Topological sort the nodes (skip start/end for step generation)
      var stepNodes = this.nodes.filter(function(n) {
        return n.type !== 'start' && n.type !== 'end';
      });

      for (var i = 0; i < stepNodes.length; i++) {
        var node = stepNodes[i];
        lines.push('[[workflow.steps]]');
        lines.push('name = "' + (node.label || 'step-' + (i + 1)) + '"');

        if (node.type === 'agent') {
          lines.push('type = "agent"');
          if (node.config.agent_name) lines.push('agent_name = "' + node.config.agent_name + '"');
          lines.push('prompt = "' + (node.config.prompt || '{{input}}') + '"');
          if (node.config.model) lines.push('model = "' + node.config.model + '"');
        } else if (node.type === 'parallel') {
          lines.push('type = "fan_out"');
          lines.push('fan_count = ' + (node.config.fan_count || 3));
        } else if (node.type === 'condition') {
          lines.push('type = "conditional"');
          lines.push('expression = "' + (node.config.expression || '') + '"');
        } else if (node.type === 'loop') {
          lines.push('type = "loop"');
          lines.push('max_iterations = ' + (node.config.max_iterations || 5));
          if (node.config.until) lines.push('until = "' + node.config.until + '"');
        } else if (node.type === 'collect') {
          lines.push('type = "collect"');
          lines.push('strategy = "' + (node.config.strategy || 'all') + '"');
        }

        // Find what this node connects to
        var outConns = self.connections.filter(function(c) { return c.from === node.id; });
        if (outConns.length === 1) {
          var target = self.getNode(outConns[0].to);
          if (target && target.type !== 'end') {
            lines.push('next = "' + target.label + '"');
          }
        } else if (outConns.length > 1 && node.type === 'condition') {
          for (var j = 0; j < outConns.length; j++) {
            var t2 = self.getNode(outConns[j].to);
            if (t2 && t2.type !== 'end') {
              var branchLabel = j === 0 ? 'true' : 'false';
              lines.push('next_' + branchLabel + ' = "' + t2.label + '"');
            }
          }
        } else if (outConns.length > 1 && node.type === 'parallel') {
          var targets = [];
          for (var k = 0; k < outConns.length; k++) {
            var t3 = self.getNode(outConns[k].to);
            if (t3 && t3.type !== 'end') targets.push('"' + t3.label + '"');
          }
          if (targets.length) lines.push('fan_targets = [' + targets.join(', ') + ']');
        }

        lines.push('');
      }

      this.tomlOutput = lines.join('\n');
      this.showTomlPreview = true;
    },

    // ── Save Workflow ────────────────────────────────────

    async saveWorkflow() {
      var steps = [];
      var stepNodes = this.nodes.filter(function(n) {
        return n.type !== 'start' && n.type !== 'end';
      });
      for (var i = 0; i < stepNodes.length; i++) {
        var node = stepNodes[i];
        var step = {
          name: node.label || 'step-' + (i + 1),
          mode: node.type === 'parallel' ? 'fan_out' : node.type === 'loop' ? 'loop' : 'sequential'
        };
        if (node.type === 'agent') {
          step.agent_name = node.config.agent_name || '';
          step.prompt = node.config.prompt || '{{input}}';
        }
        steps.push(step);
      }
      try {
        await OpenFangAPI.post('/api/workflows', {
          name: this.workflowName || 'untitled',
          description: this.workflowDescription || '',
          steps: steps
        });
        OpenFangToast.success('Workflow saved!');
        this.showSaveModal = false;
      } catch(e) {
        OpenFangToast.error('Failed to save: ' + e.message);
      }
    },

    // ── Palette drop ─────────────────────────────────────

    onPaletteDragStart: function(type, e) {
      e.dataTransfer.setData('text/plain', type);
      e.dataTransfer.effectAllowed = 'copy';
    },

    onCanvasDrop: function(e) {
      e.preventDefault();
      var type = e.dataTransfer.getData('text/plain');
      if (!type) return;
      var rect = this._getCanvasRect();
      var x = (e.clientX - rect.left) / this.zoom - this.canvasOffset.x;
      var y = (e.clientY - rect.top) / this.zoom - this.canvasOffset.y;
      this.addNode(type, x - 90, y - 35); // addNode already calls scheduleRender
    },

    onCanvasDragOver: function(e) {
      e.preventDefault();
      e.dataTransfer.dropEffect = 'copy';
    },

    // ── Auto Layout ──────────────────────────────────────

    autoLayout: function() {
      // Simple top-to-bottom layout
      var y = 40;
      var x = 200;
      for (var i = 0; i < this.nodes.length; i++) {
        this.nodes[i].x = x;
        this.nodes[i].y = y;
        y += 120;
      }
      this.scheduleRender();
    },

    // ── Clear ────────────────────────────────────────────

    clearCanvas: function() {
      this.nodes = [];
      this.connections = [];
      this.selectedNode = null;
      this.nextId = 1;
      this.addNode('start', 60, 200); // addNode already calls scheduleRender
    },

    // ── Zoom controls ────────────────────────────────────

    zoomIn: function() {
      this.zoom = Math.min(2, this.zoom + 0.1);
    },

    zoomOut: function() {
      this.zoom = Math.max(0.3, this.zoom - 0.1);
    },

    zoomReset: function() {
      this.zoom = 1;
      this.canvasOffset = { x: 0, y: 0 };
    }
  };
}
