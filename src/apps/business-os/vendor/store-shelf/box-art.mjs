// Adapted from ctox-dev components/marketing/app-package-template.ts.
// Browser-ESM canvas artwork for CTOX Business OS retail software packages.

import {
  CanvasTexture,
  LinearFilter,
  LinearMipmapLinearFilter,
  SRGBColorSpace,
} from "../three/three.module.min.js";

const WIDTH = 1024;
const HEIGHT = 1440;
const TEXTURE_SCALE = 1.5;
const SPINE_WIDTH = 1200;
const SPINE_HEIGHT = 200;

export const palettes = Object.freeze({
  ctox: { accent: "#67c9bc", background: "#102522" },
  "app-store": { accent: "#f2a24b", background: "#2a1d13" },
  importer: { accent: "#65a8eb", background: "#132238" },
  "coding-agents": { accent: "#9a8ef0", background: "#1d1935" },
  documents: { accent: "#d7bd72", background: "#282217" },
  spreadsheets: { accent: "#65c99f", background: "#10271e" },
  notes: { accent: "#ec87c5", background: "#301728" },
  calendar: { accent: "#ef8a67", background: "#311a14" },
  conversations: { accent: "#72c9d7", background: "#11282d" },
  files: { accent: "#84aee4", background: "#142137" },
  tickets: { accent: "#d8c468", background: "#292514" },
  reports: { accent: "#ed7d61", background: "#321713" },
  outbound: { accent: "#e3995b", background: "#302015" },
  shiftflow: { accent: "#9fc870", background: "#1d2915" },
  buchhaltung: { accent: "#79b5d9", background: "#122632" },
  customers: { accent: "#d68ccc", background: "#2c192a" },
  knowledge: { accent: "#b18be4", background: "#241932" },
  research: { accent: "#6eade2", background: "#112538" },
  matching: { accent: "#5fc5b8", background: "#102a27" },
  nachweise: { accent: "#cf936b", background: "#2d1d15" },
  creator: { accent: "#f1a34b", background: "#2e1d10" },
  "source-editor": { accent: "#68b8dc", background: "#102735" },
  browser: { accent: "#64c49a", background: "#10271e" },
});

const paletteValues = Object.freeze(Object.values(palettes));

function stableHash(value) {
  let hash = 2166136261;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}

/**
 * Resolve printed package colors. Unknown module ids consistently borrow one
 * of the established package palettes, keeping future apps visually related.
 */
export function resolvePackagePalette(id, accentOverride) {
  const key = String(id || "app");
  const base = palettes[key] ?? paletteValues[stableHash(key) % paletteValues.length];
  return {
    accent: typeof accentOverride === "string" && accentOverride.trim()
      ? accentOverride.trim()
      : base.accent,
    background: base.background,
  };
}

function packageCopy(template) {
  return template.locale === "en"
    ? {
        ownership: "INSTALL · VERSION · OWN",
        kicker: "APP, NOT SUBSCRIPTION.",
        features: "LOCAL-FIRST  /  VERSIONED  /  SOURCE INCLUDED",
      }
    : {
        ownership: "INSTALLIEREN · VERSIONIEREN · BESITZEN",
        kicker: "APP, NICHT ABO.",
        features: "LOCAL-FIRST  /  VERSIONIERT  /  QUELLCODE ENTHALTEN",
      };
}

function packageMonogram(template) {
  const monograms = {
    documents: "DOC",
    spreadsheets: "XLS",
    research: "WEB",
    threads: "THR",
    ctox: "C",
    knowledge: "KB",
    tickets: "TKT",
    "appsec-pentest": "PT",
  };
  return monograms[template.id] ?? String(template.title || template.id).slice(0, 3).toUpperCase();
}

function wrapText(context, text, x, y, maxWidth, lineHeight, maxLines) {
  const words = String(text || "").split(/\s+/).filter(Boolean);
  const lines = [];
  let line = "";

  for (const word of words) {
    const candidate = line ? `${line} ${word}` : word;
    if (context.measureText(candidate).width > maxWidth && line) {
      lines.push(line);
      line = word;
      if (lines.length === maxLines - 1) break;
    } else {
      line = candidate;
    }
  }
  if (line && lines.length < maxLines) lines.push(line);
  lines.forEach((value, index) => context.fillText(value, x, y + index * lineHeight));
}

function drawCoverImage(context, image, x, y, width, height, focus = 0.35) {
  const targetRatio = width / height;
  const sourceRatio = image.width / image.height;
  let sx = 0;
  let sy = 0;
  let sw = image.width;
  let sh = image.height;

  if (sourceRatio > targetRatio) {
    sw = image.height * targetRatio;
    sx = (image.width - sw) * focus;
  } else {
    sh = image.width / targetRatio;
    sy = Math.max(0, (image.height - sh) * 0.16);
  }
  context.drawImage(image, sx, sy, sw, sh, x, y, width, height);
}

function drawContainedImage(context, image, x, y, width, height) {
  const scale = Math.min(width / image.width, height / image.height);
  const drawWidth = image.width * scale;
  const drawHeight = image.height * scale;
  context.drawImage(
    image,
    x + (width - drawWidth) / 2,
    y + (height - drawHeight) / 2,
    drawWidth,
    drawHeight,
  );
}

function drawPlatformBar(context, template, width) {
  context.fillStyle = "rgba(5,8,8,.88)";
  context.fillRect(0, 0, width, 58);
  context.fillStyle = template.accent;
  context.fillRect(0, 56, width, 3);
  context.fillStyle = "#fff";
  context.font = "700 18px Arial, sans-serif";
  context.fillText(template.platform ?? "CTOX BUSINESS OS", 32, 38);
  context.fillStyle = "rgba(255,255,255,.48)";
  context.font = "600 12px ui-monospace, monospace";
  context.textAlign = "right";
  context.fillText(template.edition ?? "OWNED SOFTWARE", width - 32, 37);
  context.textAlign = "left";
}

function coverSeed(id) {
  return [...id].reduce((sum, letter) => sum + letter.charCodeAt(0), 0);
}

function drawPaperGrain(context, template, width = WIDTH, height = HEIGHT) {
  const seed = coverSeed(template.id);
  context.save();
  context.globalAlpha = 0.08;
  for (let index = 0; index < 520; index += 1) {
    const x = (index * 197 + seed * 17) % width;
    const y = (index * 89 + seed * 31) % height;
    context.fillStyle = index % 3 ? "#fff" : template.accent;
    context.fillRect(x, y, 1 + (index % 2), 1);
  }
  context.restore();
}

function drawAppMotif(context, template) {
  const accent = template.accent;
  const id = template.id;
  context.save();

  if (id === "ctox") {
    context.translate(520, 475);
    context.strokeStyle = accent;
    context.lineWidth = 4;
    for (let radius = 110; radius <= 430; radius += 80) {
      context.beginPath();
      context.ellipse(0, 0, radius, radius * 0.34, -0.22, 0, Math.PI * 2);
      context.stroke();
    }
    context.rotate(Math.PI / 4);
    for (let index = -3; index <= 3; index += 1) {
      context.strokeRect(index * 58 - 115, index * 18 - 115, 230, 230);
    }
  } else if (id === "app-store") {
    const modules = [
      [570, 170, 310, 210, -0.05],
      [500, 430, 380, 175, 0.04],
      [650, 650, 250, 245, -0.03],
      [425, 720, 180, 150, 0.08],
    ];
    modules.forEach(([x, y, width, height, rotation], index) => {
      context.save();
      context.translate(x, y);
      context.rotate(rotation);
      context.shadowColor = "rgba(0,0,0,.5)";
      context.shadowBlur = 28;
      context.fillStyle = index === 2 ? accent : "rgba(255,255,255,.09)";
      context.beginPath();
      context.roundRect(0, 0, width, height, 18);
      context.fill();
      context.restore();
    });
    context.strokeStyle = accent;
    context.lineWidth = 5;
    context.beginPath();
    context.moveTo(390, 245);
    context.bezierCurveTo(520, 330, 520, 585, 690, 690);
    context.stroke();
  } else if (id === "importer") {
    context.fillStyle = `${accent}24`;
    context.translate(130, 120);
    context.rotate(-0.18);
    for (let index = 0; index < 9; index += 1) {
      context.fillRect(index * 48, index * 76, 620 - index * 32, 24);
    }
    context.strokeStyle = accent;
    context.lineWidth = 7;
    context.strokeRect(420, 180, 330, 390);
    context.strokeRect(450, 210, 270, 330);
  } else if (id === "coding-agents") {
    context.translate(70, 160);
    context.font = "700 42px ui-monospace, monospace";
    for (let row = 0; row < 16; row += 1) {
      context.fillStyle = row % 4 === 0 ? accent : "rgba(255,255,255,.18)";
      context.fillText(
        `${row % 3 === 0 ? "→" : "·"} ${(row * 17).toString(16).padStart(4, "0")}  ${row % 2 ? "BUILD" : "REASON"}`,
        row * 12,
        row * 55,
      );
    }
  } else if (id === "documents") {
    context.translate(188, 120);
    context.rotate(-0.08);
    for (let index = 0; index < 5; index += 1) {
      context.fillStyle = index === 4 ? "#eee8d8" : `${accent}${34 + index * 16}`;
      context.shadowColor = "rgba(0,0,0,.35)";
      context.shadowBlur = 20;
      context.fillRect(index * 48, index * 36, 570, 760);
    }
    context.fillStyle = template.background;
    context.font = "700 330px Georgia, serif";
    context.fillText("D", 330, 610);
  } else if (id === "spreadsheets") {
    context.translate(96, 150);
    context.strokeStyle = "rgba(255,255,255,.2)";
    context.lineWidth = 2;
    for (let x = 0; x <= 820; x += 68) {
      context.beginPath(); context.moveTo(x, 0); context.lineTo(x, 760); context.stroke();
    }
    for (let y = 0; y <= 760; y += 58) {
      context.beginPath(); context.moveTo(0, y); context.lineTo(820, y); context.stroke();
    }
    context.fillStyle = accent;
    [[1, 2], [2, 2], [3, 2], [3, 3], [4, 3], [5, 3], [5, 4], [6, 4]].forEach(([x, y]) => {
      context.fillRect(x * 68 + 2, y * 58 + 2, 64, 54);
    });
  } else if (id === "notes") {
    context.translate(120, 165);
    const slips = [[0, 40, 360, 270, -0.08], [330, 0, 390, 300, 0.06], [110, 360, 520, 320, -0.03]];
    slips.forEach(([x, y, width, height, rotation], index) => {
      context.save(); context.translate(x, y); context.rotate(rotation);
      context.fillStyle = index === 1 ? accent : index === 2 ? "#f5e79a" : "#f4dff0";
      context.fillRect(0, 0, width, height);
      context.fillStyle = "rgba(20,20,20,.72)";
      context.font = `${index === 2 ? "italic " : ""}500 30px Georgia, serif`;
      context.fillText(index === 0 ? "remember the context" : index === 1 ? "one thought," : "kept.", 28, 72);
      context.restore();
    });
  } else if (id === "calendar") {
    context.fillStyle = accent;
    context.fillRect(82, 150, 860, 240);
    context.fillStyle = template.background;
    context.font = "800 190px Arial, sans-serif";
    context.fillText("20", 105, 345);
    context.strokeStyle = "rgba(255,255,255,.35)";
    context.lineWidth = 2;
    for (let index = 0; index < 7; index += 1) context.strokeRect(95 + index * 122, 450, 105, 105);
    context.font = "600 22px ui-monospace, monospace";
    context.fillStyle = "#fff";
    "M D M D F S S".split(" ").forEach((day, index) => context.fillText(day, 135 + index * 122, 520));
  } else if (id === "conversations") {
    const bubbles = [[90, 170, 590, 170], [340, 390, 570, 210], [120, 650, 650, 160]];
    bubbles.forEach(([x, y, width, height], index) => {
      context.fillStyle = index === 1 ? accent : "rgba(255,255,255,.12)";
      context.beginPath(); context.roundRect(x, y, width, height, 62); context.fill();
      context.fillStyle = index === 1 ? template.background : "#fff";
      context.font = "500 27px Arial, sans-serif";
      context.fillText(index === 0 ? "Context arrives." : index === 1 ? "The work continues." : "Nothing gets lost.", x + 44, y + 94);
    });
  } else if (id === "files") {
    context.translate(125, 145);
    for (let index = 0; index < 6; index += 1) {
      context.fillStyle = index === 5 ? accent : `rgba(255,255,255,${0.05 + index * 0.035})`;
      context.beginPath();
      context.moveTo(index * 54, index * 68 + 90);
      context.lineTo(index * 54 + 180, index * 68 + 90);
      context.lineTo(index * 54 + 230, index * 68 + 25);
      context.lineTo(index * 54 + 680, index * 68 + 25);
      context.lineTo(index * 54 + 680, index * 68 + 330);
      context.lineTo(index * 54, index * 68 + 330);
      context.closePath(); context.fill();
    }
  } else if (id === "tickets") {
    context.translate(80, 140);
    for (let index = 0; index < 5; index += 1) {
      const y = index * 155;
      context.fillStyle = index === 2 ? accent : "rgba(255,255,255,.1)";
      context.fillRect(0, y, 860, 116);
      context.fillStyle = template.background;
      for (let dot = 0; dot < 13; dot += 1) {
        context.beginPath(); context.arc(520 + dot * 26, y + 58, 5, 0, Math.PI * 2); context.fill();
      }
      context.font = "700 28px ui-monospace, monospace";
      context.fillText(`#0${index + 41}  ${index === 2 ? "IN REVIEW" : "QUEUED"}`, 32, y + 70);
    }
  } else if (id === "reports") {
    context.strokeStyle = accent; context.lineWidth = 10;
    context.beginPath(); context.arc(515, 470, 300, 0, Math.PI * 2); context.stroke();
    context.beginPath(); context.moveTo(515, 110); context.lineTo(515, 830); context.moveTo(155, 470); context.lineTo(875, 470); context.stroke();
    context.fillStyle = accent; context.font = "900 280px Arial, sans-serif"; context.fillText("!", 445, 575);
  } else if (id === "outbound") {
    context.translate(165, 510); context.strokeStyle = accent; context.lineWidth = 5;
    for (let radius = 90; radius < 620; radius += 95) {
      context.beginPath(); context.arc(0, 0, radius, -1.05, 1.05); context.stroke();
    }
    for (let angle = -0.9; angle <= 0.9; angle += 0.3) {
      context.beginPath(); context.moveTo(0, 0); context.lineTo(Math.cos(angle) * 760, Math.sin(angle) * 760); context.stroke();
    }
    context.fillStyle = "#fff"; context.beginPath(); context.arc(0, 0, 34, 0, Math.PI * 2); context.fill();
  } else if (id === "shiftflow") {
    context.translate(80, 160);
    const widths = [520, 690, 340, 760, 470];
    widths.forEach((width, index) => {
      context.fillStyle = index === 3 ? accent : "rgba(255,255,255,.12)";
      context.beginPath(); context.roundRect(index % 2 ? 150 : 0, index * 145, width, 76, 38); context.fill();
    });
    context.strokeStyle = accent; context.lineWidth = 4; context.beginPath(); context.arc(690, 650, 150, 0, Math.PI * 2); context.stroke();
    context.beginPath(); context.moveTo(690, 650); context.lineTo(690, 550); context.moveTo(690, 650); context.lineTo(780, 690); context.stroke();
  } else if (id === "buchhaltung") {
    context.fillStyle = "#eee9dc"; context.fillRect(100, 130, 820, 770);
    context.strokeStyle = "rgba(20,30,35,.22)"; context.lineWidth = 2;
    for (let y = 220; y < 860; y += 68) {
      context.beginPath(); context.moveTo(130, y); context.lineTo(890, y); context.stroke();
    }
    context.fillStyle = accent; context.fillRect(670, 130, 6, 770);
    context.fillStyle = template.background; context.font = "700 130px Georgia, serif"; context.fillText("Σ", 150, 330);
    context.font = "600 26px ui-monospace, monospace"; context.fillText("BALANCE / 20", 710, 820);
  } else if (id === "customers") {
    const nodes = [[160, 230], [460, 150], [760, 300], [260, 570], [620, 650], [820, 800]];
    context.strokeStyle = `${accent}80`; context.lineWidth = 4;
    nodes.forEach(([x, y], index) => nodes.slice(index + 1, index + 3).forEach(([x2, y2]) => {
      context.beginPath(); context.moveTo(x, y); context.lineTo(x2, y2); context.stroke();
    }));
    nodes.forEach(([x, y], index) => {
      context.fillStyle = index === 3 ? accent : "#f0e8ef";
      context.beginPath(); context.arc(x, y, index === 3 ? 62 : 38, 0, Math.PI * 2); context.fill();
    });
  } else if (id === "knowledge") {
    context.translate(140, 130);
    for (let index = 0; index < 7; index += 1) {
      context.fillStyle = index % 2 ? accent : "rgba(255,255,255,.16)";
      context.fillRect(index * 85, index * 58, 520, 92);
    }
    context.strokeStyle = "#fff"; context.lineWidth = 4; context.beginPath(); context.moveTo(590, 80); context.bezierCurveTo(820, 210, 360, 500, 760, 760); context.stroke();
  } else if (id === "research") {
    context.strokeStyle = "rgba(255,255,255,.2)"; context.lineWidth = 3;
    for (let index = 0; index < 12; index += 1) {
      const x = 100 + (index * 137) % 820;
      const y = 130 + (index * 197) % 720;
      context.beginPath(); context.moveTo(510, 470); context.lineTo(x, y); context.stroke();
      context.fillStyle = index === 7 ? accent : "#fff";
      context.beginPath(); context.arc(x, y, index === 7 ? 22 : 9, 0, Math.PI * 2); context.fill();
    }
    context.strokeStyle = accent; context.lineWidth = 14; context.beginPath(); context.arc(510, 470, 180, 0, Math.PI * 2); context.stroke();
    context.beginPath(); context.moveTo(640, 600); context.lineTo(820, 780); context.stroke();
  } else if (id === "matching") {
    context.fillStyle = accent;
    context.beginPath(); context.arc(345, 460, 230, -Math.PI / 2, Math.PI / 2); context.fill();
    context.fillStyle = "rgba(255,255,255,.9)";
    context.beginPath(); context.arc(680, 460, 230, Math.PI / 2, Math.PI * 1.5); context.fill();
    context.strokeStyle = "#fff"; context.lineWidth = 5;
    for (let index = 0; index < 9; index += 1) {
      context.beginPath(); context.moveTo(300, 210 + index * 65); context.bezierCurveTo(470, 180 + index * 65, 540, 720 - index * 55, 720, 720 - index * 55); context.stroke();
    }
  } else if (id === "nachweise") {
    context.fillStyle = "#e9dfcf"; context.save(); context.translate(130, 160); context.rotate(-0.05); context.fillRect(0, 0, 760, 720);
    context.strokeStyle = accent; context.lineWidth = 18; context.strokeRect(80, 100, 600, 360);
    context.fillStyle = accent; context.font = "900 86px ui-monospace, monospace"; context.fillText("VERIFIED", 120, 315);
    context.font = "600 24px ui-monospace, monospace"; context.fillText("HASH 7A9F / SOURCE 04", 110, 560); context.restore();
  } else if (id === "creator") {
    context.translate(110, 130); context.strokeStyle = "rgba(255,255,255,.22)"; context.lineWidth = 2;
    for (let x = 0; x < 820; x += 55) {
      context.beginPath(); context.moveTo(x, 0); context.lineTo(x, 780); context.stroke();
    }
    for (let y = 0; y < 780; y += 55) {
      context.beginPath(); context.moveTo(0, y); context.lineTo(820, y); context.stroke();
    }
    context.fillStyle = accent;
    [[1, 2, 3, 2], [5, 1, 4, 3], [2, 6, 2, 4], [6, 6, 5, 2]].forEach(([x, y, width, height]) => {
      context.fillRect(x * 55, y * 55, width * 55, height * 55);
    });
  } else if (id === "source-editor") {
    context.fillStyle = "#071015"; context.fillRect(70, 120, 884, 790);
    context.font = "600 30px ui-monospace, monospace";
    ["export const work = {", "  source: true,", "  inspectable: true,", "  owned: true,", "};", "", "work.run();"].forEach((line, index) => {
      context.fillStyle = index === 1 || index === 6 ? accent : "rgba(255,255,255,.72)";
      context.fillText(line, 120, 220 + index * 82);
    });
    context.fillStyle = accent; context.fillRect(96, 165, 5, 620);
  } else if (id === "browser") {
    context.strokeStyle = accent; context.lineWidth = 20;
    context.beginPath(); context.arc(515, 485, 330, 0, Math.PI * 2); context.stroke();
    context.lineWidth = 4;
    for (let radius = 90; radius < 330; radius += 80) {
      context.beginPath(); context.arc(515, 485, radius, 0, Math.PI * 2); context.stroke();
    }
    for (let y = 240; y <= 730; y += 120) {
      context.beginPath(); context.moveTo(205, y); context.bezierCurveTo(390, y - 70, 650, y + 70, 825, y); context.stroke();
    }
    context.fillStyle = "#fff"; context.beginPath(); context.moveTo(565, 390); context.lineTo(780, 510); context.lineTo(590, 570); context.closePath(); context.fill();
  } else {
    context.translate(512, 470);
    context.strokeStyle = accent;
    context.lineWidth = 5;
    for (let radius = 120; radius < 520; radius += 80) {
      context.beginPath(); context.arc(0, 0, radius, 0, Math.PI * 2); context.stroke();
    }
  }
  context.restore();
}

function drawFront(context, template, images) {
  const heroArtwork = template.heroArtwork ? images.get(template.heroArtwork) : undefined;
  const base = context.createLinearGradient(0, 0, WIDTH, HEIGHT);
  base.addColorStop(0, template.background);
  base.addColorStop(0.7, template.background);
  base.addColorStop(1, "#060808");
  context.fillStyle = base;
  context.fillRect(0, 0, WIDTH, HEIGHT);

  drawPaperGrain(context, template);
  drawAppMotif(context, template);
  drawPlatformBar(context, template, WIDTH);

  if (heroArtwork) {
    const positions = {
      ctox: [760, 155, 150], "app-store": [670, 620, 170], importer: [790, 105, 120],
      "coding-agents": [780, 760, 135], documents: [760, 760, 130], spreadsheets: [760, 170, 130],
      notes: [750, 165, 125], calendar: [760, 760, 135], conversations: [780, 160, 120],
      files: [760, 710, 140], tickets: [760, 760, 125], reports: [760, 760, 130],
      outbound: [760, 150, 130], shiftflow: [120, 760, 130], buchhaltung: [760, 760, 130],
      customers: [100, 760, 130], knowledge: [760, 700, 135], research: [760, 740, 135],
      matching: [765, 745, 135], nachweise: [755, 735, 140], creator: [760, 720, 135],
      "source-editor": [760, 750, 130], browser: [120, 760, 130],
    };
    const [iconX, iconY, iconSize] = positions[template.id] ?? [760, 760, 130];
    const centerX = iconX + iconSize / 2;
    const centerY = iconY + iconSize / 2;
    context.beginPath();
    context.fillStyle = `${template.accent}28`;
    context.arc(centerX, centerY, iconSize / 2, 0, Math.PI * 2);
    context.fill();
    context.strokeStyle = `${template.accent}b8`;
    context.lineWidth = 3;
    context.stroke();
    drawContainedImage(context, heroArtwork, iconX + 18, iconY + 18, iconSize - 36, iconSize - 36);
  }
  context.textAlign = "left";
  context.textBaseline = "alphabetic";

  const editorialTitle = template.id === "app-store" || template.id === "source-editor";
  context.fillStyle = editorialTitle ? template.accent : "#fff";
  context.font = editorialTitle ? "800 112px Arial Narrow, Arial, sans-serif" : "650 78px Arial, sans-serif";
  wrapText(context, template.title, 64, editorialTitle ? 1085 : 1110, 875, editorialTitle ? 102 : 78, 2);

  context.fillStyle = "rgba(255,255,255,.58)";
  context.font = "600 18px ui-monospace, monospace";
  context.fillText(`${template.category.toUpperCase()}  /  ${packageCopy(template).ownership}`, 66, 1348);
}

function drawScreenshotFrame(context, image, x, y, width, height, accent) {
  context.fillStyle = "#080b0b";
  context.fillRect(x, y, width, height);
  if (image) drawCoverImage(context, image, x, y, width, height, 0.5);
  context.strokeStyle = "rgba(255,255,255,.26)";
  context.lineWidth = 2;
  context.strokeRect(x, y, width, height);
  context.fillStyle = accent;
  context.fillRect(x, y + height - 6, Math.min(64, width * 0.18), 6);
}

function drawNoScreenshotBack(context, template) {
  context.save();
  context.translate(64, 310);
  context.fillStyle = "rgba(255,255,255,.045)";
  context.fillRect(0, 0, 896, 650);
  context.strokeStyle = `${template.accent}88`;
  context.lineWidth = 3;
  context.strokeRect(0, 0, 896, 650);
  context.fillStyle = template.accent;
  context.font = "800 240px Arial, sans-serif";
  context.textAlign = "center";
  context.fillText(packageMonogram(template), 448, 355);
  context.fillStyle = "rgba(255,255,255,.52)";
  context.font = "600 19px ui-monospace, monospace";
  context.fillText(packageCopy(template).ownership, 448, 455);
  context.strokeStyle = "rgba(255,255,255,.12)";
  context.beginPath(); context.moveTo(220, 520); context.lineTo(676, 520); context.stroke();
  context.fillStyle = "rgba(255,255,255,.78)";
  context.font = "500 28px Arial, sans-serif";
  context.textAlign = "left";
  wrapText(context, template.description, 94, 585, 708, 38, 2);
  context.restore();
}

function drawBack(context, template, images) {
  context.fillStyle = template.background;
  context.fillRect(0, 0, WIDTH, HEIGHT);
  drawPaperGrain(context, template);
  drawPlatformBar(context, template, WIDTH);

  context.fillStyle = template.accent;
  context.font = "600 20px ui-monospace, monospace";
  context.fillText(packageCopy(template).kicker, 64, 142);
  context.fillStyle = "#fff";
  context.font = "650 49px Arial, sans-serif";
  wrapText(context, template.description, 64, 207, 890, 55, 2);

  const screenshots = template.screenshots.filter(Boolean);
  if (screenshots.length === 0) {
    drawNoScreenshotBack(context, template);
  } else {
    const [primary, secondary, tertiary] = screenshots;
    const secondarySource = secondary ?? primary;
    const tertiarySource = tertiary ?? primary;
    drawScreenshotFrame(context, images.get(primary), 64, 314, 896, 472, template.accent);
    drawScreenshotFrame(context, images.get(secondarySource), 64, 814, 432, 238, template.accent);
    drawScreenshotFrame(context, images.get(tertiarySource), 528, 814, 432, 238, template.accent);

    context.fillStyle = "rgba(255,255,255,.76)";
    context.font = "400 28px Arial, sans-serif";
    wrapText(context, template.description, 64, 1115, 896, 39, 3);
  }

  context.strokeStyle = "rgba(255,255,255,.15)";
  context.beginPath();
  context.moveTo(64, 1255);
  context.lineTo(960, 1255);
  context.stroke();

  context.fillStyle = template.accent;
  context.font = "650 17px ui-monospace, monospace";
  context.fillText(template.featureLine ?? packageCopy(template).features, 64, 1310);
  context.fillStyle = "rgba(255,255,255,.34)";
  context.font = "500 14px ui-monospace, monospace";
  context.fillText("CTOX PACKAGE FORMAT 1.0", 64, 1368);
  context.textAlign = "right";
  context.fillText("INSTALL · ROLLBACK · INSPECT", 960, 1368);
  context.textAlign = "left";
}

function drawSpine(context, template, width, height) {
  const lightSpines = new Set(["documents", "spreadsheets", "notes", "calendar", "nachweise"]);
  const light = lightSpines.has(template.id);
  const titleColor = light ? "#151b1c" : "#fff";
  context.fillStyle = light ? (template.id === "notes" ? "#efe5cc" : "#e8e8df") : template.background;
  context.fillRect(0, 0, width, height);
  drawPaperGrain(context, template, width, height);
  context.fillStyle = template.accent;
  if (["app-store", "outbound", "creator"].includes(template.id)) {
    context.fillRect(0, 0, width, 14);
  } else if (["coding-agents", "conversations", "customers"].includes(template.id)) {
    for (let index = 0; index < 4; index += 1) context.fillRect(index * 17, 0, 10, height);
  } else {
    context.fillRect(0, 0, 18, height);
  }
  context.fillStyle = titleColor;
  const titleSize = template.title.length > 20 ? 36 : template.title.length > 14 ? 40 : 46;
  const serif = ["documents", "knowledge", "research"].includes(template.id);
  context.font = `650 ${titleSize}px ${serif ? "Georgia, serif" : "Arial, sans-serif"}`;
  const titleX = ["app-store", "calendar", "reports"].includes(template.id) ? 86 : 58;
  context.fillText(template.title, titleX, 122);
  context.fillStyle = light ? "rgba(15,25,25,.48)" : "rgba(255,255,255,.45)";
  context.font = "700 19px ui-monospace, monospace";
  context.textAlign = "right";
  context.fillText("CTOX BUSINESS OS", width - 54, 120);
  context.textAlign = "left";
}

function createArtworkCanvas(width, height) {
  if (typeof OffscreenCanvas === "function") return new OffscreenCanvas(width, height);
  if (typeof document !== "undefined" && document.createElement) {
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    return canvas;
  }
  throw new Error("createAppPackageTexture requires Canvas or OffscreenCanvas support");
}

/**
 * Create one asynchronously updating package-art texture.
 *
 * @param {{id:string,title:string,category:string,description:string,accent:string,background:string,heroArtwork?:string,screenshots?:string[],platform?:string,edition?:string,featureLine?:string,locale?:'de'|'en'}} template
 * @param {'front'|'back'|'spine'} panel
 * @returns {CanvasTexture}
 */
export function createAppPackageTexture(template, panel) {
  if (!["front", "back", "spine"].includes(panel)) {
    throw new TypeError(`Unknown package panel: ${panel}`);
  }
  const normalized = {
    ...template,
    id: String(template.id || "app"),
    title: String(template.title || template.id || "App"),
    category: String(template.category || "Business OS"),
    description: String(template.description || ""),
    screenshots: Array.isArray(template.screenshots) ? template.screenshots.filter(Boolean) : [],
    locale: template.locale === "en" ? "en" : "de",
  };
  const designWidth = panel === "spine" ? SPINE_WIDTH : WIDTH;
  const designHeight = panel === "spine" ? SPINE_HEIGHT : HEIGHT;
  const canvas = createArtworkCanvas(designWidth * TEXTURE_SCALE, designHeight * TEXTURE_SCALE);
  const context = canvas.getContext("2d");
  const texture = new CanvasTexture(canvas);
  texture.colorSpace = SRGBColorSpace;
  texture.anisotropy = 16;
  texture.minFilter = LinearMipmapLinearFilter;
  texture.magFilter = LinearFilter;
  texture.generateMipmaps = true;

  if (!context) return texture;

  const images = new Map();
  const pendingImages = new Set();
  let disposed = false;
  const render = () => {
    if (disposed) return;
    context.setTransform(1, 0, 0, 1, 0, 0);
    context.clearRect(0, 0, canvas.width, canvas.height);
    context.setTransform(TEXTURE_SCALE, 0, 0, TEXTURE_SCALE, 0, 0);
    if (panel === "front") drawFront(context, normalized, images);
    else if (panel === "back") drawBack(context, normalized, images);
    else drawSpine(context, normalized, designWidth, designHeight);
    texture.needsUpdate = true;
  };

  texture.addEventListener("dispose", () => {
    disposed = true;
    pendingImages.forEach((image) => {
      image.onload = null;
      image.onerror = null;
    });
    pendingImages.clear();
    images.clear();
  });

  render();

  if (panel !== "spine" && typeof Image === "function") {
    const sources = panel === "front"
      ? (normalized.heroArtwork ? [normalized.heroArtwork] : [])
      : [...new Set(normalized.screenshots)];
    sources.forEach((source) => {
      const image = new Image();
      pendingImages.add(image);
      image.onload = () => {
        pendingImages.delete(image);
        if (disposed) return;
        images.set(source, image);
        render();
      };
      image.onerror = () => pendingImages.delete(image);
      image.src = source;
    });
  }

  return texture;
}
