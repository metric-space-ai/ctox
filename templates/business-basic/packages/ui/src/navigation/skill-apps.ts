export type SkillAppModuleId =
  | "documents"
  | "content"
  | "developer"
  | "deployment"
  | "security"
  | "integrations"
  | "research"
  | "support";

export type SkillAppSubmodule = {
  id: string;
  label: string;
  href: string;
  resourceTypes: string[];
};

export type SkillAppModule = {
  id: SkillAppModuleId;
  label: string;
  href: string;
  summary: string;
  submodules: SkillAppSubmodule[];
};

export type SkillAppBinding = {
  skillId: string;
  pack: string;
  title: string;
  moduleId: SkillAppModuleId;
  submoduleId: string;
  route: string;
  sourcePath: string;
  description: string;
  capabilities: string[];
};

export const skillAppModuleDefinitions: SkillAppModule[] = [
  {
    id: "documents",
    label: "Documents",
    href: "/app/documents/library",
    summary: "Document, spreadsheet, slide, transcript, and drawing workspaces.",
    submodules: [
      { id: "library", label: "Library", href: "/app/documents/library", resourceTypes: ["document", "pdf", "docx"] },
      { id: "spreadsheets", label: "Spreadsheets", href: "/app/documents/spreadsheets", resourceTypes: ["spreadsheet", "csv", "xlsx"] },
      { id: "slides", label: "Slides", href: "/app/documents/slides", resourceTypes: ["presentation", "pptx"] },
      { id: "drawings", label: "Drawings", href: "/app/documents/drawings", resourceTypes: ["technical_drawing", "review"] },
      { id: "transcripts", label: "Transcripts", href: "/app/documents/transcripts", resourceTypes: ["transcript", "audio", "video"] }
    ]
  },
  {
    id: "content",
    label: "Content Studio",
    href: "/app/content/assets",
    summary: "Visual, audio, video, screenshot, and design production.",
    submodules: [
      { id: "assets", label: "Assets", href: "/app/content/assets", resourceTypes: ["asset", "screenshot"] },
      { id: "images", label: "Images", href: "/app/content/images", resourceTypes: ["image", "variant"] },
      { id: "video", label: "Video", href: "/app/content/video", resourceTypes: ["video", "shot"] },
      { id: "voice", label: "Voice", href: "/app/content/voice", resourceTypes: ["voiceover", "audio"] },
      { id: "design", label: "Design", href: "/app/content/design", resourceTypes: ["figma_file", "design_system"] },
      { id: "web", label: "Web UI", href: "/app/content/web", resourceTypes: ["frontend", "prototype"] }
    ]
  },
  {
    id: "developer",
    label: "Developer Studio",
    href: "/app/developer/apps",
    summary: "Application, framework, notebook, source-control, and quality workflows.",
    submodules: [
      { id: "apps", label: "Apps", href: "/app/developer/apps", resourceTypes: ["application", "service"] },
      { id: "frameworks", label: "Frameworks", href: "/app/developer/frameworks", resourceTypes: ["framework", "template"] },
      { id: "notebooks", label: "Notebooks", href: "/app/developer/notebooks", resourceTypes: ["notebook", "experiment"] },
      { id: "source-control", label: "Source Control", href: "/app/developer/source-control", resourceTypes: ["pull_request", "review", "commit"] },
      { id: "quality", label: "Quality", href: "/app/developer/quality", resourceTypes: ["browser_test", "smoke_test"] }
    ]
  },
  {
    id: "deployment",
    label: "Deployment",
    href: "/app/deployment/overview",
    summary: "Provider deployment paths and release readiness.",
    submodules: [
      { id: "overview", label: "Overview", href: "/app/deployment/overview", resourceTypes: ["deployment", "release"] },
      { id: "vercel", label: "Vercel", href: "/app/deployment/vercel", resourceTypes: ["deployment", "preview"] },
      { id: "cloudflare", label: "Cloudflare", href: "/app/deployment/cloudflare", resourceTypes: ["worker", "pages_site"] },
      { id: "netlify", label: "Netlify", href: "/app/deployment/netlify", resourceTypes: ["site", "deploy"] },
      { id: "render", label: "Render", href: "/app/deployment/render", resourceTypes: ["service", "blueprint"] }
    ]
  },
  {
    id: "security",
    label: "Security",
    href: "/app/security/overview",
    summary: "Secure coding, ownership, and threat-model workspaces.",
    submodules: [
      { id: "overview", label: "Overview", href: "/app/security/overview", resourceTypes: ["security_review", "risk"] },
      { id: "best-practices", label: "Best Practices", href: "/app/security/best-practices", resourceTypes: ["security_review", "finding"] },
      { id: "ownership", label: "Ownership", href: "/app/security/ownership", resourceTypes: ["ownership_map", "bus_factor"] },
      { id: "threat-models", label: "Threat Models", href: "/app/security/threat-models", resourceTypes: ["threat_model", "mitigation"] }
    ]
  },
  {
    id: "integrations",
    label: "Integration Hub",
    href: "/app/integrations/overview",
    summary: "Linear, Notion, and connected operational system workflows.",
    submodules: [
      { id: "overview", label: "Overview", href: "/app/integrations/overview", resourceTypes: ["integration", "connector"] },
      { id: "linear", label: "Linear", href: "/app/integrations/linear", resourceTypes: ["issue", "project"] },
      { id: "notion", label: "Notion", href: "/app/integrations/notion", resourceTypes: ["page", "database", "spec"] }
    ]
  },
  {
    id: "research",
    label: "Research Desk",
    href: "/app/research/desk",
    summary: "Documentation, research synthesis, and reference workflows.",
    submodules: [
      { id: "desk", label: "Desk", href: "/app/research/desk", resourceTypes: ["research_item", "brief"] },
      { id: "openai-docs", label: "OpenAI Docs", href: "/app/research/openai-docs", resourceTypes: ["reference", "api_doc"] },
      { id: "notion-research", label: "Notion Research", href: "/app/research/notion-research", resourceTypes: ["notion_page", "synthesis"] }
    ]
  },
  {
    id: "support",
    label: "Support Desk",
    href: "/app/support/tickets",
    summary: "Support tickets, incidents, monitoring, and vendor-specific workflows.",
    submodules: [
      { id: "tickets", label: "Tickets", href: "/app/support/tickets", resourceTypes: ["ticket", "customer_context"] },
      { id: "monitoring", label: "Monitoring", href: "/app/support/monitoring", resourceTypes: ["incident", "error", "health_signal"] },
      { id: "zammad", label: "Zammad", href: "/app/support/zammad", resourceTypes: ["zammad_ticket", "api_client"] }
    ]
  }
];

export const skillAppBindings: SkillAppBinding[] = [
  skill("doc", "content", "Documents", "documents", "library", "Document authoring and DOCX editing.", ["docx", "formatting", "review"]),
  skill("pdf", "content", "PDF", "documents", "library", "PDF reading, generation, and layout checks.", ["pdf", "rendering", "extraction"]),
  skill("spreadsheet", "content", "Spreadsheets", "documents", "spreadsheets", "Spreadsheet creation, analysis, and charts.", ["xlsx", "csv", "formulas"]),
  skill("slides", "content", "Slides", "documents", "slides", "Presentation deck production and review.", ["pptx", "storytelling", "export"]),
  skill("technical-drawing-review", "content", "Technical Drawing Review", "documents", "drawings", "Technical drawing inspection and review.", ["drawings", "qa", "annotation"]),
  skill("transcribe", "content", "Transcribe", "documents", "transcripts", "Audio and video transcription workflows.", ["audio", "diarization", "transcript"]),
  skill("screenshot", "content", "Screenshot", "content", "assets", "Desktop and app screenshots for evidence and assets.", ["capture", "visual-proof"]),
  skill("imagegen", "content", "Image Generation", "content", "images", "AI image generation and bitmap asset variants.", ["image", "variant", "asset"]),
  skill("sora", "content", "Sora", "content", "video", "AI video generation and video queues.", ["video", "storyboard", "asset"]),
  skill("speech", "content", "Speech", "content", "voice", "Text-to-speech and voiceover generation.", ["audio", "voiceover", "accessibility"]),
  skill("figma", "design", "Figma", "content", "design", "Figma context, screenshots, and asset extraction.", ["figma", "design-context"]),
  skill("figma-implement-design", "design", "Figma Implementation", "content", "design", "Figma-to-code implementation workflows.", ["figma", "implementation"]),
  skill("frontend-skill", "development", "Frontend Skill", "content", "web", "Frontend experience design and implementation guidance.", ["frontend", "ui", "ux"]),
  skill("aspnet-core", "development", "ASP.NET Core", "developer", "frameworks", "ASP.NET Core implementation and review.", ["dotnet", "web-api", "blazor"]),
  skill("chatgpt-apps", "development", "ChatGPT Apps", "developer", "apps", "ChatGPT Apps SDK implementation workflows.", ["mcp", "widget", "apps-sdk"]),
  skill("develop-web-game", "development", "Web Game Development", "developer", "apps", "Browser game implementation and testing loop.", ["game", "playwright", "canvas"]),
  skill("jupyter-notebook", "development", "Jupyter Notebook", "developer", "notebooks", "Notebook scaffolding and experiment workflows.", ["notebook", "experiment"]),
  skill("nextjs-postgres-port", "development", "Next.js Postgres Port", "developer", "frameworks", "Next.js and Postgres migration workflows.", ["nextjs", "postgres", "migration"]),
  skill("winui-app", "development", "WinUI App", "developer", "apps", "WinUI 3 desktop application workflows.", ["desktop", "windows", "xaml"]),
  skill("gh-address-comments", "git", "Address PR Comments", "developer", "source-control", "GitHub review comment resolution.", ["github", "review", "pr"]),
  skill("gh-fix-ci", "git", "Fix CI", "developer", "quality", "GitHub Actions failure triage and fixes.", ["ci", "github-actions", "logs"]),
  skill("yeet", "git", "Publish PR", "developer", "source-control", "Stage, commit, push, and open a pull request.", ["git", "github", "pr"]),
  skill("playwright", "testing", "Playwright", "developer", "quality", "Browser automation and UI flow checks.", ["browser", "automation", "smoke"]),
  skill("playwright-interactive", "testing", "Playwright Interactive", "developer", "quality", "Persistent browser debugging sessions.", ["browser", "debug", "interactive"]),
  skill("cloudflare-deploy", "deploy", "Cloudflare Deploy", "deployment", "cloudflare", "Cloudflare Workers and Pages deployment.", ["cloudflare", "workers", "pages"]),
  skill("netlify-deploy", "deploy", "Netlify Deploy", "deployment", "netlify", "Netlify preview and production deploys.", ["netlify", "preview", "deploy"]),
  skill("render-deploy", "deploy", "Render Deploy", "deployment", "render", "Render Blueprint and service deployment.", ["render", "blueprint", "service"]),
  skill("vercel-deploy", "deploy", "Vercel Deploy", "deployment", "vercel", "Vercel preview and production deploys.", ["vercel", "preview", "deploy"]),
  skill("security-best-practices", "security", "Security Best Practices", "security", "best-practices", "Secure coding reviews for supported stacks.", ["security", "review", "code"]),
  skill("security-ownership-map", "security", "Security Ownership Map", "security", "ownership", "Repository ownership and bus-factor analysis.", ["ownership", "risk", "git-history"]),
  skill("security-threat-model", "security", "Security Threat Model", "security", "threat-models", "Repository-grounded threat modeling.", ["threat-model", "mitigation", "architecture"]),
  skill("linear", "integration", "Linear", "integrations", "linear", "Linear issue and project workflows.", ["linear", "issues", "projects"]),
  skill("notion-knowledge-capture", "integration", "Notion Knowledge Capture", "integrations", "notion", "Capture decisions and knowledge into Notion.", ["notion", "knowledge", "wiki"]),
  skill("notion-meeting-intelligence", "integration", "Notion Meeting Intelligence", "integrations", "notion", "Meeting preparation and follow-up from Notion context.", ["notion", "meetings", "agenda"]),
  skill("notion-spec-to-implementation", "integration", "Notion Spec to Implementation", "integrations", "notion", "Turn Notion specs into implementation plans.", ["notion", "spec", "planning"]),
  skill("openai-docs", "reference", "OpenAI Docs", "research", "openai-docs", "Official OpenAI product and API documentation workflows.", ["openai", "reference", "docs"]),
  skill("notion-research-documentation", "integration", "Notion Research Documentation", "research", "notion-research", "Research synthesis across Notion sources.", ["notion", "research", "documentation"]),
  skill("sentry", "integration", "Sentry", "support", "monitoring", "Production error inspection and health triage.", ["sentry", "errors", "monitoring"]),
  skill("zammad-rest", "vendor", "Zammad REST", "support", "zammad", "Zammad REST API support workflows.", ["zammad", "tickets", "api"]),
  skill("zammad-printengine-monitoring-sim", "vendor", "Zammad Print Engine Monitoring", "support", "monitoring", "Zammad print-engine monitoring simulation.", ["zammad", "monitoring", "simulation"])
];

export function getSkillAppsForModule(moduleId: string, submoduleId?: string) {
  return skillAppBindings.filter((binding) => {
    if (binding.moduleId !== moduleId) return false;
    return submoduleId ? binding.submoduleId === submoduleId : true;
  });
}

export function findSkillApp(skillId: string) {
  return skillAppBindings.find((binding) => binding.skillId === skillId);
}

function skill(
  skillId: string,
  pack: string,
  title: string,
  moduleId: SkillAppModuleId,
  submoduleId: string,
  description: string,
  capabilities: string[]
): SkillAppBinding {
  return {
    skillId,
    pack,
    title,
    moduleId,
    submoduleId,
    route: `/app/${moduleId}/${submoduleId}?skillId=${encodeURIComponent(skillId)}&panel=skill&drawer=right`,
    sourcePath: `skills/packs/${pack}/${skillId}/SKILL.md`,
    description,
    capabilities
  };
}
