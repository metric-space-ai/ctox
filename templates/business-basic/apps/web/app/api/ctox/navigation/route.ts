import { NextResponse } from "next/server";
import { businessModules, skillAppBindings } from "@ctox-business/ui";
import { readBusinessOsNavigationState } from "../../../../lib/ctox-core-bridge";

export async function GET() {
  const activation = await readBusinessOsNavigationState();
  const enabledModuleSet = new Set(activation.enabledModules);
  const enabledSkillSet = new Set(activation.enabledSkills);
  const modules = businessModules.filter((module) => enabledModuleSet.has(module.id));
  const skillApps = skillAppBindings.filter((skill) => enabledSkillSet.has(skill.skillId));

  return NextResponse.json({
    ok: true,
    activation,
    modules,
    skillApps
  });
}
