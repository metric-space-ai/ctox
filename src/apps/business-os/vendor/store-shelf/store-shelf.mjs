// Adapted from ctox-dev components/marketing/app-store-showcase.tsx.
// Self-contained browser-ESM 3D retail-box shelf for CTOX Business OS.

import {
  ACESFilmicToneMapping,
  Color,
  DirectionalLight,
  EdgesGeometry,
  Group,
  HemisphereLight,
  LineBasicMaterial,
  LineSegments,
  MathUtils,
  Mesh,
  MeshPhysicalMaterial,
  PCFShadowMap,
  PerspectiveCamera,
  PlaneGeometry,
  Raycaster,
  Scene,
  ShadowMaterial,
  SRGBColorSpace,
  Texture,
  Vector2,
  WebGLRenderer,
} from "../three/three.module.min.js";
import { RoundedBoxGeometry } from "../three/RoundedBoxGeometry.js";
import { createAppPackageTexture, resolvePackagePalette } from "./box-art.mjs";

function damp(current, target, smoothing, delta) {
  return MathUtils.lerp(current, target, 1 - Math.exp(-smoothing * delta));
}

function normalizeApps(apps) {
  if (!Array.isArray(apps)) return [];
  return apps.map((app, index) => ({
    id: String(app?.id || `app-${index + 1}`),
    title: String(app?.title || app?.id || `App ${index + 1}`),
    category: String(app?.category || "Business OS"),
    description: String(app?.description || ""),
    accentOverride: typeof app?.accentOverride === "string" ? app.accentOverride : undefined,
    screenshots: Array.isArray(app?.screenshots) ? app.screenshots.filter(Boolean).map(String) : [],
    iconUrl: app?.iconUrl ? String(app.iconUrl) : "",
  }));
}

function createShelfPositions(apps) {
  let groupCount = 0;
  return apps.map((app, index) => {
    if (index > 0 && apps[index - 1]?.category !== app.category) groupCount += 1;
    return index + groupCount * 0.56;
  });
}

function createRetailBox(app, index, locale) {
  const palette = resolvePackagePalette(app.id, app.accentOverride);
  const template = {
    id: app.id,
    title: app.title,
    category: app.category,
    description: app.description,
    accent: palette.accent,
    background: palette.background,
    heroArtwork: app.iconUrl || undefined,
    screenshots: app.screenshots,
    locale,
  };
  const frontTexture = createAppPackageTexture(template, "front");
  const spineTexture = createAppPackageTexture(template, "spine");
  const dark = new Color(palette.background).multiplyScalar(0.54);
  const sideMaterial = new MeshPhysicalMaterial({
    color: dark,
    roughness: 0.66,
    clearcoat: 0.16,
    clearcoatRoughness: 0.5,
  });
  const spineMaterial = new MeshPhysicalMaterial({
    map: spineTexture,
    roughness: 0.7,
    clearcoat: 0.12,
  });
  const frontMaterial = new MeshPhysicalMaterial({
    map: frontTexture,
    roughness: 0.58,
    clearcoat: 0.18,
  });

  const group = new Group();
  group.userData = {
    id: app.id,
    index,
    materials: [sideMaterial, spineMaterial, frontMaterial],
  };

  // Retail software boxes vary slightly in footprint and depth, like a real shelf.
  const sizeScale = 0.93 + ((index * 7) % 5) * 0.028;
  const boxWidth = 3.9 * sizeScale;
  const boxHeight = 5.5 * sizeScale;
  const boxDepth = 0.58 + ((index * 11) % 5) * 0.018;
  const boxGeometry = new RoundedBoxGeometry(
    boxWidth,
    boxHeight,
    boxDepth,
    3,
    0.035 + (index % 3) * 0.01,
  );
  const body = new Mesh(boxGeometry, sideMaterial);
  body.castShadow = true;
  body.receiveShadow = true;
  group.add(body);

  // Printed front and back panels sit flush on the rigid box.
  const panelGeometry = new PlaneGeometry(boxWidth - 0.16, boxHeight - 0.18);
  const frontPanel = new Mesh(panelGeometry, frontMaterial);
  frontPanel.position.z = boxDepth / 2 + 0.004;
  group.add(frontPanel);
  const backPanel = new Mesh(panelGeometry, sideMaterial);
  backPanel.position.z = -(boxDepth / 2 + 0.004);
  backPanel.rotation.y = Math.PI;
  group.add(backPanel);

  // The printed spine is the face pointing toward the camera in shelf mode.
  const spinePanel = new Mesh(
    new PlaneGeometry(boxWidth - 0.16, boxDepth - 0.035),
    spineMaterial,
  );
  spinePanel.position.set(0, -(boxHeight / 2 + 0.004), 0);
  spinePanel.rotation.x = Math.PI / 2;
  group.add(spinePanel);

  const edges = new LineSegments(
    new EdgesGeometry(boxGeometry, 28),
    new LineBasicMaterial({
      color: 0xffffff,
      transparent: true,
      opacity: 0.13,
    }),
  );
  edges.scale.setScalar(1.002);
  edges.renderOrder = 3;
  group.add(edges);
  return group;
}

function disposeObjectTrees(roots) {
  const geometries = new Set();
  const materials = new Set();
  const textures = new Set();

  roots.forEach((root) => {
    root.traverse((object) => {
      if (object.geometry && !geometries.has(object.geometry)) {
        geometries.add(object.geometry);
        object.geometry.dispose();
      }
      const objectMaterials = object.material
        ? (Array.isArray(object.material) ? object.material : [object.material])
        : [];
      objectMaterials.forEach((material) => {
        if (!material || materials.has(material)) return;
        materials.add(material);
        if (material.map instanceof Texture && !textures.has(material.map)) {
          textures.add(material.map);
          material.map.dispose();
        }
        material.dispose();
      });
    });
    root.removeFromParent();
  });
}

function requireElement(value, name) {
  if (!value || typeof value.addEventListener !== "function") {
    throw new TypeError(`createStoreShelf requires a valid ${name}`);
  }
  return value;
}

/**
 * Create the animated retail-box shelf inside an existing canvas.
 * The caller owns every surrounding control, rail, and detail element.
 *
 * @param {HTMLCanvasElement} canvas
 * @param {{apps:Array,locale:'de'|'en',scrollContainer:Element,track:Element,stage:Element,onSelect:(id:string)=>void,onHoverChange?:(id:string|null)=>void}} options
 */
export function createStoreShelf(canvas, {
  apps,
  locale = "de",
  scrollContainer,
  track,
  stage,
  onSelect,
  onHoverChange,
} = {}) {
  requireElement(canvas, "canvas");
  requireElement(scrollContainer, "scrollContainer");
  requireElement(track, "track");
  requireElement(stage, "stage");
  if (typeof onSelect !== "function") {
    throw new TypeError("createStoreShelf requires onSelect(id)");
  }

  const language = locale === "en" ? "en" : "de";
  const scene = new Scene();
  const camera = new PerspectiveCamera(40, 1, 0.1, 100);
  camera.position.set(0, 0.2, 11);

  const renderer = new WebGLRenderer({
    canvas,
    antialias: true,
    alpha: true,
    powerPreference: "high-performance",
  });
  renderer.outputColorSpace = SRGBColorSpace;
  renderer.toneMapping = ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1.06;
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = PCFShadowMap;
  renderer.setClearColor(0x000000, 0);

  scene.add(new HemisphereLight(0xffffff, 0x101414, 2.15));
  const keyLight = new DirectionalLight(0xffffff, 3.4);
  keyLight.position.set(-4, 7, 9);
  keyLight.castShadow = true;
  scene.add(keyLight);
  const rimLight = new DirectionalLight(0x83d9ce, 2.1);
  rimLight.position.set(7, -2, 6);
  scene.add(rimLight);

  const floor = new Mesh(
    new PlaneGeometry(30, 24),
    new ShadowMaterial({ color: 0x000000, opacity: 0.34 }),
  );
  floor.position.set(0, -4.25, -0.5);
  floor.rotation.x = -Math.PI / 2;
  floor.receiveShadow = true;
  scene.add(floor);

  const raycaster = new Raycaster();
  const pointer = new Vector2();
  let appData = [];
  let cases = [];
  let shelfPositions = [];
  let maxShelfPosition = 0;
  let scrollProgress = 0;
  let nearestIndex = 0;
  let selectedId = null;
  let hoveredId = null;
  let lastSelected = null;
  let frame = 0;
  let destroyed = false;
  let lastFrameTime = globalThis.performance.now();

  const setHover = (id) => {
    const next = id ?? null;
    if (next === hoveredId) return;
    hoveredId = next;
    if (typeof onHoverChange === "function") onHoverChange(next);
  };

  const findNearestIndex = () => {
    if (shelfPositions.length === 0) return 0;
    return shelfPositions.reduce(
      (closest, position, index) =>
        Math.abs(position - scrollProgress) < Math.abs(shelfPositions[closest] - scrollProgress)
          ? index
          : closest,
      0,
    );
  };

  const updateScroll = (force = false) => {
    if (selectedId && !force) return;
    const available = Math.max(1, track.offsetHeight - scrollContainer.clientHeight);
    const ratio = scrollContainer.scrollTop / available;
    scrollProgress = MathUtils.clamp(ratio, 0, 1) * maxShelfPosition;
    nearestIndex = findNearestIndex();
  };

  const resize = () => {
    if (destroyed) return;
    const width = Math.max(1, stage.clientWidth);
    const height = Math.max(1, stage.clientHeight);
    const pixelRatio = Math.min(globalThis.devicePixelRatio || 1, 1.75);
    renderer.setPixelRatio(pixelRatio);
    renderer.setSize(width, height, false);
    camera.aspect = width / height;
    camera.fov = width < 760 ? 52 : 40;
    camera.updateProjectionMatrix();
  };

  const hitTest = (event) => {
    const rect = canvas.getBoundingClientRect();
    if (!rect.width || !rect.height) {
      setHover(null);
      return null;
    }
    pointer.x = ((event.clientX - rect.left) / rect.width) * 2 - 1;
    pointer.y = -((event.clientY - rect.top) / rect.height) * 2 + 1;
    raycaster.setFromCamera(pointer, camera);
    const hit = raycaster.intersectObjects(cases, true)[0];
    let id = null;
    if (hit) {
      let object = hit.object;
      while (object && !object.userData.id) object = object.parent;
      id = object?.userData.id ?? null;
    }
    setHover(id);
    return id;
  };

  const select = (id) => {
    if (destroyed) return;
    const key = String(id);
    if (!appData.some((app) => app.id === key)) return;
    selectedId = key;
    setHover(null);
  };

  const deselect = () => {
    if (destroyed) return;
    selectedId = null;
    setHover(null);
    updateScroll(true);
  };

  const onPointerMove = (event) => {
    if (!selectedId) hitTest(event);
  };
  const onPointerLeave = () => setHover(null);
  const onScroll = () => updateScroll();
  const onClick = (event) => {
    if (selectedId) return;
    const id = hitTest(event);
    if (!id) return;
    select(id);
    onSelect(id);
  };

  const rebuildCases = (nextApps) => {
    setHover(null);
    disposeObjectTrees(cases);
    appData = normalizeApps(nextApps);
    cases = appData.map((app, index) => {
      const retailBox = createRetailBox(app, index, language);
      retailBox.position.set(0, 2.4 - index * 1.58, 0);
      retailBox.rotation.x = -1.2;
      retailBox.scale.set(1.52, 0.56, 1);
      scene.add(retailBox);
      return retailBox;
    });
    shelfPositions = createShelfPositions(appData);
    maxShelfPosition = shelfPositions.at(-1) ?? 0;
    if (selectedId && !appData.some((app) => app.id === selectedId)) selectedId = null;
    updateScroll(true);
  };

  const setApps = (nextApps) => {
    if (destroyed) return;
    rebuildCases(nextApps);
  };

  const currentIndex = () => nearestIndex;

  const render = () => {
    if (destroyed) return;
    const now = globalThis.performance.now();
    const delta = Math.min((now - lastFrameTime) / 1000, 0.05);
    lastFrameTime = now;
    const selectedIndex = cases.findIndex((item) => item.userData.id === selectedId);
    const compact = stage.clientWidth < 900;
    const mobile = stage.clientWidth < 600;

    if (selectedId !== lastSelected) {
      lastSelected = selectedId;
      setHover(null);
    }

    cases.forEach((retailBox, index) => {
      const isSelected = retailBox.userData.id === selectedId;
      const inDetail = selectedId !== null;
      const isHovered = hoveredId === retailBox.userData.id;
      let targetX = isHovered ? -0.55 : 0;
      const shelfTop = 2.35;
      let targetY = shelfTop - (shelfPositions[index] - scrollProgress) * 1.06;
      if (isHovered) targetY += 0.16;
      let targetZ = isHovered ? 0.82 : 0;
      let rotationX = -1.2 + (isHovered ? pointer.y * 0.045 : 0);
      let rotationY = isHovered ? -pointer.x * 0.08 : 0;
      let rotationZ = isHovered ? -0.035 - pointer.x * 0.025 : 0;
      let scaleX = compact ? 0.88 : 1.52;
      let scaleY = compact ? 0.42 : 0.56;
      let scaleZ = 1;

      if (inDetail && isSelected) {
        targetX = mobile ? 0 : compact ? -1.65 : -2.65;
        targetY = mobile ? 1.35 : 0;
        targetZ = 2.1;
        rotationX = compact ? -0.05 : -0.035;
        rotationY = -0.18;
        rotationZ = compact ? 0 : -0.035;
        scaleX = mobile ? 0.68 : compact ? 0.74 : 0.94;
        scaleY = mobile ? 0.68 : compact ? 0.74 : 0.94;
        scaleZ = mobile ? 0.68 : compact ? 0.74 : 0.94;
      } else if (inDetail) {
        const direction = index < selectedIndex ? -1 : 1;
        targetX = direction * 16;
        targetY = 2.35 - (shelfPositions[index] - scrollProgress) * 1.06;
        targetZ = -5;
        rotationZ = direction * 0.18;
        scaleX = 1.2;
        scaleY = 0.42;
      }

      retailBox.position.x = damp(retailBox.position.x, targetX, 7.8, delta);
      retailBox.position.y = damp(retailBox.position.y, targetY, 7.8, delta);
      retailBox.position.z = damp(retailBox.position.z, targetZ, 7.8, delta);
      retailBox.rotation.x = damp(retailBox.rotation.x, rotationX, 7.4, delta);
      retailBox.rotation.y = damp(retailBox.rotation.y, rotationY, 7.1, delta);
      retailBox.rotation.z = damp(retailBox.rotation.z, rotationZ, 7.4, delta);
      retailBox.scale.x = damp(retailBox.scale.x, scaleX, 7.4, delta);
      retailBox.scale.y = damp(retailBox.scale.y, scaleY, 7.4, delta);
      retailBox.scale.z = damp(retailBox.scale.z, scaleZ, 7.4, delta);
    });

    renderer.render(scene, camera);
    frame = globalThis.requestAnimationFrame(render);
  };

  const resizeObserver = new ResizeObserver(resize);
  resizeObserver.observe(stage);
  canvas.addEventListener("pointermove", onPointerMove);
  canvas.addEventListener("pointerleave", onPointerLeave);
  canvas.addEventListener("click", onClick);
  scrollContainer.addEventListener("scroll", onScroll, { passive: true });

  rebuildCases(apps);
  resize();
  updateScroll(true);
  render();

  const destroy = () => {
    if (destroyed) return;
    destroyed = true;
    globalThis.cancelAnimationFrame(frame);
    resizeObserver.disconnect();
    canvas.removeEventListener("pointermove", onPointerMove);
    canvas.removeEventListener("pointerleave", onPointerLeave);
    canvas.removeEventListener("click", onClick);
    scrollContainer.removeEventListener("scroll", onScroll);
    setHover(null);
    disposeObjectTrees(cases);
    cases = [];
    disposeObjectTrees([floor]);
    renderer.dispose();
  };

  return {
    select,
    deselect,
    setApps,
    currentIndex,
    destroy,
  };
}
