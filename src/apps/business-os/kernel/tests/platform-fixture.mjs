// Explicit test installation. Declares only the collections exercised by each fixture;
// native exports and the kernel's normal foreign-read guard remain authoritative.
export async function installFixtureApp(kernel, id, {own=[], platform=[]}={}) {
  const collections=Object.fromEntries(own.map(name=>[name,{schema:{type:'object',properties:{id:{type:'string'}}}}]));
  const foreign_collections=Object.fromEntries(platform.map(name=>[name,{owner_app:'ctox-platform',ops:['read','write']}]));
  await kernel.sim.publishAppVersion(id,'1',{collections,manifestExtra:{foreign_collections}});
}
