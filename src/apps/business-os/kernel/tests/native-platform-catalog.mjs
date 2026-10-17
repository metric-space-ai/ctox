import {PLATFORM_MANIFEST} from '../platform-manifest.generated.mjs';
// Wire/lifecycle fixture catalog: Core-confirmed versions, independent of bundled app schemas.
// Schema validation itself is exercised by e35/e47 and the native store contracts.
export const nativePlatformCatalog = {
 business_apps: [{id:'ctox-platform',active_ver:'platform-native'}],
 business_app_versions: [{id:'ctox-platform@platform-native',app_id:'ctox-platform',app_ver:'platform-native',manifest:{...PLATFORM_MANIFEST,collections:Object.fromEntries(['business_commands','files','ctox_queue_tasks'].map(name=>[name,{schema:{version:3,type:'object'}}]))}}],
};
export async function publishNativePlatformCatalog(page) {
 for (const [collection,rows] of Object.entries(nativePlatformCatalog))
  await page(collection,rows.map(data=>({id:data.id,seq:'0',rev:'native-catalog',data})));
}
