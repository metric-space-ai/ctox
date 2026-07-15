import {
  AdditiveBlending,
  BoxGeometry,
  CanvasTexture,
  Color,
  ForceGraph3D,
  Group,
  Mesh,
  MeshLambertMaterial,
  SphereGeometry,
  Sprite,
  SpriteMaterial,
  SpriteText,
  Vector3,
} from '../../vendor/research-graph.mjs';

const BACKGROUND = '#0c0e0f';
const MUTED = '#66717a';

export function createResearchGraph(host, options = {}) {
  if (!(host instanceof HTMLElement)) throw new TypeError('Research graph host is required.');
  assertWebGlSupport();

  const resources = new Set();
  const nodeObjects = new Map();
  const neighborIds = new Map();
  const haloMaterials = new Map();
  const cameraFitTimers = new Set();
  let projection = normalizeProjection(options.projection);
  let dimensions = options.dimensions === 2 ? 2 : 3;
  let selectedId = '';
  let hoveredId = '';
  let autoRotateEnabled = options.autoRotate !== false;
  let settledFitPending = true;
  let disposed = false;

  const graph = ForceGraph3D({
    controlType: 'orbit',
    rendererConfig: {
      alpha: false,
      antialias: projection.nodes.length <= 240,
      powerPreference: 'high-performance',
    },
  })(host)
    .backgroundColor(BACKGROUND)
    .showNavInfo(false)
    .numDimensions(dimensions)
    .nodeId('id')
    .nodeLabel((node) => `${node.label}\n${Math.round((node.importance || 0) * 100)}% relevance`)
    .nodeVal((node) => node.visualSize || 2)
    .nodeThreeObject((node) => buildNodeObject(node))
    .nodeThreeObjectExtend(false)
    .linkColor((link) => linkColor(link))
    .linkWidth((link) => linkWidth(link))
    .linkCurvature((link) => link.curvature || 0)
    .linkCurveRotation((link) => seededRotation(link.id))
    .linkDirectionalParticles((link) => reduceMotion() ? 0 : (link.particles || 0))
    .linkDirectionalParticleWidth((link) => Math.max(0.7, (link.visualWidth || 1) * 0.62))
    .linkDirectionalParticleSpeed((link) => 0.002 + Math.min(0.004, (link.weight || 1) * 0.00015))
    .linkDirectionalParticleColor((link) => link.color || MUTED)
    .linkOpacity(0.5)
    .onNodeHover((node) => {
      hoveredId = node?.id || '';
      applyFocusState();
      host.style.cursor = node ? 'pointer' : 'grab';
      options.onNodeHover?.(node || null);
    })
    .onNodeClick((node) => {
      select(node?.id || '', { focus: true });
      options.onNodeClick?.(node || null);
    })
    .onBackgroundClick(() => {
      select('', { focus: false });
      options.onBackgroundClick?.();
    })
    .onEngineStop(() => {
      // The initial force pass can expand far beyond the camera bounds after
      // the eager first fit. Re-fit once the layout has actually settled so a
      // freshly replicated graph never opens as an apparently empty canvas.
      if (settledFitPending) {
        settledFitPending = false;
        fit(900);
      }
      options.onSettled?.();
    });

  const controls = graph.controls?.();
  if (controls) {
    controls.enableDamping = true;
    controls.dampingFactor = 0.09;
    controls.rotateSpeed = 0.55;
    controls.zoomSpeed = 0.8;
    controls.panSpeed = 0.5;
    controls.autoRotateSpeed = 0.38;
  }

  configureForces();
  graph.graphData(cloneProjection(projection));
  setAutoRotate(options.autoRotate !== false);
  resize();

  const resizeObserver = new ResizeObserver(resize);
  resizeObserver.observe(host);
  const visibilityHandler = () => {
    if (document.hidden) graph.pauseAnimation?.();
    else graph.resumeAnimation?.();
  };
  document.addEventListener('visibilitychange', visibilityHandler);

  const api = {
    setData(nextProjection) {
      if (disposed) return;
      projection = normalizeProjection(nextProjection);
      settledFitPending = true;
      rebuildAdjacency();
      disposeNodeObjects();
      graph.graphData(cloneProjection(projection));
      configureForces();
      graph.d3ReheatSimulation?.();
      scheduleInitialCameraFits();
    },
    setDimensions(nextDimensions) {
      if (disposed) return;
      dimensions = nextDimensions === 2 ? 2 : 3;
      settledFitPending = true;
      graph.numDimensions(dimensions);
      configureForces();
      graph.d3ReheatSimulation?.();
      setAutoRotate(autoRotateEnabled);
      if (dimensions === 2) graph.cameraPosition({ x: 0, y: 0, z: 520 }, { x: 0, y: 0, z: 0 }, 700);
      window.setTimeout(() => fit(560), 180);
    },
    setAutoRotate,
    search(query) {
      const normalized = String(query || '').trim().toLocaleLowerCase();
      if (!normalized) {
        select('', { focus: false });
        return null;
      }
      const node = projection.nodes.find((candidate) => candidate.label.toLocaleLowerCase().includes(normalized));
      if (node) select(node.id, { focus: true });
      return node || null;
    },
    select,
    zoomIn() {
      dolly(0.76);
    },
    zoomOut() {
      dolly(1.3);
    },
    fit,
    reset() {
      select('', { focus: false });
      setAutoRotate(autoRotateEnabled);
      fit(720);
    },
    pause() {
      graph.pauseAnimation?.();
    },
    resume() {
      graph.resumeAnimation?.();
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      resizeObserver.disconnect();
      document.removeEventListener('visibilitychange', visibilityHandler);
      for (const timer of cameraFitTimers) window.clearTimeout(timer);
      cameraFitTimers.clear();
      disposeNodeObjects();
      for (const material of haloMaterials.values()) {
        material.map?.dispose?.();
        material.dispose?.();
      }
      haloMaterials.clear();
      graph._destructor?.();
      host.replaceChildren();
    },
    graph,
  };

  rebuildAdjacency();
  scheduleInitialCameraFits();
  return api;

  function buildNodeObject(node) {
    const group = new Group();
    group.userData.graphNodeId = node.id;
    const size = Math.max(1.5, node.visualSize || 2);
    const geometry = node.primary
      ? new BoxGeometry(size * 1.42, size * 1.42, Math.max(1.3, size * 0.48))
      : new SphereGeometry(size * 0.58, 14, 10);
    const material = new MeshLambertMaterial({
      color: new Color(node.color || '#58a9d8'),
      emissive: new Color(node.color || '#58a9d8'),
      emissiveIntensity: node.primary ? 0.28 : 0.1,
      transparent: true,
      opacity: 0.94,
      depthWrite: true,
    });
    resources.add(geometry);
    resources.add(material);
    const mesh = new Mesh(geometry, material);
    mesh.userData.role = 'node';
    group.add(mesh);

    if (node.primary || (node.importance || 0) > 0.32) {
      const halo = new Sprite(haloMaterial(node.color || '#58a9d8'));
      const haloSize = size * (node.primary ? 5.6 : 3.8);
      halo.scale.set(haloSize, haloSize, 1);
      halo.userData.role = 'halo';
      halo.renderOrder = -1;
      group.add(halo);
    }

    const label = new SpriteText(node.label || node.id);
    label.color = node.color || '#d6eaf3';
    label.textHeight = Math.max(3.6, node.labelSize || 5);
    label.fontFace = 'Inter, ui-sans-serif, system-ui, sans-serif';
    label.fontWeight = node.primary ? 520 : 440;
    label.strokeWidth = node.primary ? 1.8 : 1.25;
    label.strokeColor = BACKGROUND;
    label.padding = [0.5, 1.1];
    label.backgroundColor = 'rgba(12,14,15,0.12)';
    label.center.set(0, 0.5);
    label.position.set(size * 0.92, 0, node.primary ? size * 0.28 : 0);
    label.material.depthWrite = false;
    label.material.depthTest = dimensions !== 2;
    label.material.transparent = true;
    label.material.opacity = node.primary ? 0.98 : 0.9;
    label.renderOrder = 10;
    label.userData.role = 'label';
    resources.add(label.material);
    resources.add(label.material.map);
    group.add(label);
    nodeObjects.set(node.id, group);
    return group;
  }

  function haloMaterial(color) {
    if (haloMaterials.has(color)) return haloMaterials.get(color);
    const canvas = document.createElement('canvas');
    canvas.width = 128;
    canvas.height = 128;
    const context = canvas.getContext('2d');
    const gradient = context.createRadialGradient(64, 64, 2, 64, 64, 62);
    gradient.addColorStop(0, withAlpha(color, 0.65));
    gradient.addColorStop(0.23, withAlpha(color, 0.24));
    gradient.addColorStop(1, withAlpha(color, 0));
    context.fillStyle = gradient;
    context.fillRect(0, 0, 128, 128);
    const texture = new CanvasTexture(canvas);
    const material = new SpriteMaterial({
      map: texture,
      color: 0xffffff,
      transparent: true,
      opacity: 0.82,
      depthWrite: false,
      blending: AdditiveBlending,
    });
    haloMaterials.set(color, material);
    return material;
  }

  function configureForces() {
    const factor = dimensions === 2 ? 0.65 : 1;
    const linkForce = graph.d3Force?.('link');
    linkForce?.distance?.((link) => {
      const source = nodeValue(link.source);
      const target = nodeValue(link.target);
      return (source?.cluster === target?.cluster ? 52 : 148) * factor;
    });
    linkForce?.strength?.((link) => Math.min(0.74, 0.12 + (link.weight || 1) * 0.035));
    graph.d3Force?.('charge')?.strength?.((node) => -45 - (node.visualSize || 2) * 7.5);
    graph.d3Force?.('center')?.strength?.(0.055);
    graph.cooldownTime?.(8000);
    graph.cooldownTicks?.(360);
  }

  function rebuildAdjacency() {
    neighborIds.clear();
    projection.nodes.forEach((node) => neighborIds.set(node.id, new Set([node.id])));
    projection.links.forEach((link) => {
      const source = nodeId(link.source);
      const target = nodeId(link.target);
      neighborIds.get(source)?.add(target);
      neighborIds.get(target)?.add(source);
    });
  }

  function applyFocusState() {
    const focusId = hoveredId || selectedId;
    const visible = focusId ? neighborIds.get(focusId) || new Set([focusId]) : null;
    for (const [id, object] of nodeObjects) {
      const active = !visible || visible.has(id);
      const direct = id === focusId;
      object.children.forEach((child) => {
        if (!child.material) return;
        if (child.userData.role === 'halo') child.material.opacity = active ? (direct ? 1 : 0.66) : 0.035;
        else if (child.userData.role === 'label') child.material.opacity = active ? (direct ? 1 : 0.9) : 0.1;
        else child.material.opacity = active ? 0.96 : 0.14;
      });
    }
    graph.linkColor(linkColor);
    graph.linkWidth(linkWidth);
  }

  function linkColor(link) {
    const focusId = hoveredId || selectedId;
    if (!focusId) return link.color || MUTED;
    const source = nodeId(link.source);
    const target = nodeId(link.target);
    return source === focusId || target === focusId ? (link.color || '#d7edf5') : '#20272b';
  }

  function linkWidth(link) {
    const focusId = hoveredId || selectedId;
    if (!focusId) return link.visualWidth || 0.25;
    const source = nodeId(link.source);
    const target = nodeId(link.target);
    return source === focusId || target === focusId ? Math.max(1.2, link.visualWidth || 0.25) : 0.08;
  }

  function select(nodeIdValue, { focus = false } = {}) {
    selectedId = nodeIdValue || '';
    applyFocusState();
    if (!focus || !selectedId) return;
    const node = graph.graphData().nodes.find((candidate) => candidate.id === selectedId);
    if (!node || !Number.isFinite(node.x)) return;
    const position = new Vector3(node.x || 0, node.y || 0, dimensions === 2 ? 0 : (node.z || 0));
    const distance = dimensions === 2 ? 190 : 150;
    const length = Math.max(1, position.length());
    const ratio = 1 + distance / length;
    graph.cameraPosition(
      dimensions === 2
        ? { x: position.x, y: position.y, z: distance }
        : { x: position.x * ratio, y: position.y * ratio, z: position.z * ratio },
      { x: position.x, y: position.y, z: position.z },
      900,
    );
  }

  function dolly(factor) {
    const camera = graph.camera?.();
    if (!camera) return;
    const target = graph.controls?.()?.target || new Vector3(0, 0, 0);
    const next = camera.position.clone().sub(target).multiplyScalar(factor).add(target);
    graph.cameraPosition({ x: next.x, y: next.y, z: next.z }, target, 320);
  }

  function fit(duration = 700) {
    graph.resumeAnimation?.();
    graph.zoomToFit?.(duration, 36);
    window.setTimeout(() => {
      if (disposed) return;
      dolly(0.72);
      graph.refresh?.();
    }, duration + 45);
  }

  function scheduleInitialCameraFits() {
    // ForceGraph assigns coordinates incrementally. A single eager fit can
    // therefore frame an empty origin and later leave the expanded graph
    // outside the camera. These bounded checkpoints cover both quick and
    // large live projections without depending on an engine-stop event.
    scheduleCameraFit(420, 620);
    scheduleCameraFit(2200, 760);
    scheduleCameraFit(9200, 900);
  }

  function scheduleCameraFit(delay, duration) {
    const timer = window.setTimeout(() => {
      cameraFitTimers.delete(timer);
      if (!disposed) fit(duration);
    }, delay);
    cameraFitTimers.add(timer);
  }

  function setAutoRotate(enabled) {
    autoRotateEnabled = Boolean(enabled);
    const graphControls = graph.controls?.();
    if (!graphControls) return;
    graphControls.autoRotate = autoRotateEnabled && !reduceMotion() && dimensions === 3;
  }

  function resize() {
    if (disposed) return;
    const rect = host.getBoundingClientRect();
    if (rect.width < 2 || rect.height < 2) return;
    graph.width(Math.floor(rect.width));
    graph.height(Math.floor(rect.height));
  }

  function disposeNodeObjects() {
    nodeObjects.clear();
    for (const resource of resources) resource?.dispose?.();
    resources.clear();
  }
}

function assertWebGlSupport() {
  const canvas = document.createElement('canvas');
  const context = canvas.getContext('webgl2', { failIfMajorPerformanceCaveat: true })
    || canvas.getContext('webgl2')
    || canvas.getContext('webgl', { failIfMajorPerformanceCaveat: true })
    || canvas.getContext('webgl');
  if (!context) throw new Error('WebGL ist auf diesem Gerät nicht verfügbar.');
}

function normalizeProjection(value) {
  return {
    nodes: Array.isArray(value?.nodes) ? value.nodes : [],
    links: Array.isArray(value?.links) ? value.links : [],
  };
}

function cloneProjection(projection) {
  return {
    nodes: projection.nodes.map((node) => ({ ...node })),
    links: projection.links.map((link) => ({
      ...link,
      source: nodeId(link.source),
      target: nodeId(link.target),
    })),
  };
}

function nodeId(value) {
  return typeof value === 'object' && value ? value.id : String(value || '');
}

function nodeValue(value) {
  return typeof value === 'object' && value ? value : null;
}

function seededRotation(value) {
  let hash = 0;
  for (const character of String(value || '')) hash = Math.imul(31, hash) + character.charCodeAt(0) | 0;
  return ((hash >>> 0) % 6283) / 1000;
}

function reduceMotion() {
  return window.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches === true;
}

function withAlpha(color, alpha) {
  const hex = String(color || '#58a9d8').replace('#', '');
  const value = hex.length === 3 ? hex.split('').map((part) => part + part).join('') : hex.padEnd(6, '0').slice(0, 6);
  const number = Number.parseInt(value, 16);
  const red = number >> 16 & 255;
  const green = number >> 8 & 255;
  const blue = number & 255;
  return `rgba(${red}, ${green}, ${blue}, ${alpha})`;
}
