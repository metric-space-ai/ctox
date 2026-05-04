# CTOX Public Website Repo Template

This folder is intentionally standalone. Copy it into its own Git repository when
the public website should be deployed independently from the Business OS.

The website can read published page metadata from Marketing / Website through:

```text
CTOX_BUSINESS_OS_URL=https://business.example.com
GET $CTOX_BUSINESS_OS_URL/api/public/website/pages
```

Business OS access through website login is deployment-controlled:

```text
# Business OS deployment
CTOX_BUSINESS_OS_ACCESS_MODE=local   # default, only local Business OS login
CTOX_BUSINESS_OS_ACCESS_MODE=hybrid  # local login plus website login with role
CTOX_BUSINESS_OS_ACCESS_MODE=website # website login with role only
CTOX_WEBSITE_AUTH_SECRET=shared-secret
CTOX_WEBSITE_SESSION_COOKIE=ctox_website_session
CTOX_BUSINESS_OS_ROLE=business_os_user
CTOX_BUSINESS_OS_ADMIN_ROLE=business_os_admin

# Public website deployment
WEBSITE_AUTH_SECRET=shared-secret
BUSINESS_OS_URL=https://business.example.com
NEXT_PUBLIC_BUSINESS_OS_URL=https://business.example.com
```

Only website users whose signed session has `business_os_user`,
`business_os_admin`, `business_os:access`, or `business_os:admin` can enter
`/app/*` on the Business OS. Normal customer users remain limited to the public
website.

The login implementation here is a small reference adapter. Replace the
username/password check with the production website identity provider and keep
the signed session contract.
