import {PLATFORM_MANIFEST} from '../platform-manifest.generated.mjs';
// Core-confirmed catalog, with the exact production platform schema (E61).
export const nativePlatformCatalog = {
 business_apps: [{id:'ctox-platform',active_ver:'platform-native'}],
 business_app_versions: [{id:'ctox-platform@platform-native',app_id:'ctox-platform',app_ver:'platform-native',manifest:PLATFORM_MANIFEST}],
};
export async function publishNativePlatformCatalog(page) {
 for (const [collection,rows] of Object.entries(nativePlatformCatalog))
  await page(collection,rows.map(data=>({id:data.id,seq:'0',rev:'native-catalog',data})));
}
