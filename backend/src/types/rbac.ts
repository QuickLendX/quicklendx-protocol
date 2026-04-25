export const ADMIN_ROLES = [
  "support",
  "operations_admin",
  "super_admin",
] as const;

export type AdminRole = (typeof ADMIN_ROLES)[number];

export const SUPPORT_READ_ROLES: readonly AdminRole[] = [
  "support",
  "operations_admin",
  "super_admin",
];

export const OPERATIONS_WRITE_ROLES: readonly AdminRole[] = [
  "operations_admin",
  "super_admin",
];

export const SUPER_ADMIN_ONLY_ROLES: readonly AdminRole[] = ["super_admin"];
