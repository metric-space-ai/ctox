"use strict";

const ACCESS_REVOKED_LAUNCH_ERROR = /ctox\.dev managed instance is not launchable: needs_auth|ctox\.dev launch token failed: (400|401|403|404)/;

function summarizeAccessRevocationBlock({ postRevocationInstance, launchAfterRevocationError }) {
  const postRevocationStatus = postRevocationInstance?.status || "removed";
  if (postRevocationInstance && postRevocationStatus !== "needs_auth") {
    throw new Error(`access revocation target tenant did not become needs_auth or disappear: ${postRevocationStatus}`);
  }
  if (!launchAfterRevocationError) {
    throw new Error("access revocation launch unexpectedly succeeded after viewer role change");
  }
  if (!ACCESS_REVOKED_LAUNCH_ERROR.test(launchAfterRevocationError)) {
    throw new Error(`access revocation launch failed for unexpected reason: ${launchAfterRevocationError}`);
  }
  return {
    postRevocationStatus,
    launchAfterRevocationBlocked: true,
    launchAfterRevocationError,
  };
}

module.exports = {
  ACCESS_REVOKED_LAUNCH_ERROR,
  summarizeAccessRevocationBlock,
};
