// SPDX-License-Identifier: AGPL-3.0-only
//! Tenant/group/space effective role resolution.
//!
//! The three role layers are deliberately separate: tenant roles decide
//! administration, group roles describe a group, and space roles are the only
//! values that gate content. This module implements the documented precedence:
//! explicit space member wins, then group-derived roles, then owner type.

use crate::governance::{GroupRole, Principal, PrincipalKind, PrincipalType, SpaceRole};
use crate::store::{GroupMembership, SpaceRecord, Store};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpaceAccess {
    pub role: SpaceRole,
    /// True when a tenant owner/admin acts without an explicit space
    /// membership. Content reads/writes in this mode must be audited.
    pub break_glass: bool,
}

pub fn principal_type(kind: PrincipalKind) -> PrincipalType {
    match kind {
        PrincipalKind::Agent | PrincipalKind::Service => PrincipalType::Agent,
        PrincipalKind::Human => PrincipalType::User,
    }
}

fn group_member_default(space: &SpaceRecord) -> SpaceRole {
    let parsed: serde_json::Value = serde_json::from_str(&space.settings).unwrap_or_default();
    match parsed
        .get("group_member_space_role")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("viewer")
    {
        "editor" => SpaceRole::Editor,
        _ => SpaceRole::Viewer,
    }
}

fn max_role(current: Option<SpaceRole>, candidate: SpaceRole) -> Option<SpaceRole> {
    Some(match current {
        Some(existing) if existing >= candidate => existing,
        _ => candidate,
    })
}

/// Resolve the effective space role, loading group memberships itself.
pub async fn effective_space_role(
    store: &Arc<dyn Store>,
    principal: &Principal,
    space: &SpaceRecord,
) -> Result<Option<SpaceAccess>, crate::error::StoreError> {
    if principal.tenant_role.is_admin() {
        return Ok(Some(SpaceAccess {
            role: SpaceRole::SpaceOwner,
            break_glass: true,
        }));
    }
    let memberships = store
        .list_group_memberships(&principal.tenant_id, &principal.principal_id)
        .await?;
    effective_space_role_with(store, principal, space, &memberships).await
}

pub async fn effective_space_role_with(
    store: &Arc<dyn Store>,
    principal: &Principal,
    space: &SpaceRecord,
    memberships: &[GroupMembership],
) -> Result<Option<SpaceAccess>, crate::error::StoreError> {
    if principal.tenant_role.is_admin() {
        return Ok(Some(SpaceAccess {
            role: SpaceRole::SpaceOwner,
            break_glass: true,
        }));
    }

    // Explicit principal membership always wins.
    let direct_type = principal_type(principal.kind);
    if let Some(member) = store
        .get_space_member(&space.space_id, direct_type, &principal.principal_id)
        .await?
    {
        return Ok(Some(SpaceAccess {
            role: member.role,
            break_glass: false,
        }));
    }

    let mut best: Option<SpaceRole> = None;

    // User-owned spaces grant the owner the space_owner role even if the
    // membership row is missing (migration compatibility).
    if space.owner_type == "user" && space.owner_id == principal.principal_id {
        best = max_role(best, SpaceRole::SpaceOwner);
    }

    for membership in memberships {
        if space.owner_type == "group" && space.owner_id == membership.group.group_id {
            let role = match membership.member.role {
                GroupRole::GroupAdmin => SpaceRole::SpaceAdmin,
                GroupRole::GroupMember => group_member_default(space),
            };
            best = max_role(best, role);
        }
        if let Some(member) = store
            .get_space_member(
                &space.space_id,
                PrincipalType::Group,
                &membership.group.group_id,
            )
            .await?
        {
            best = max_role(best, member.role);
        }
    }

    Ok(best.map(|role| SpaceAccess {
        role,
        break_glass: false,
    }))
}

/// Space ids the principal may discover/read through membership, group or
/// ownership. Tenant owner/admin get every non-deleted space.
pub async fn allowed_space_ids(
    store: &Arc<dyn Store>,
    principal: &Principal,
) -> Result<Vec<String>, crate::error::StoreError> {
    let spaces = store.list_spaces(&principal.tenant_id).await?;
    if principal.tenant_role.is_admin() {
        return Ok(spaces
            .into_iter()
            .filter(|space| space.status != "deleted")
            .map(|space| space.space_id)
            .collect());
    }
    let memberships = store
        .list_group_memberships(&principal.tenant_id, &principal.principal_id)
        .await?;
    let mut ids = Vec::new();
    for space in spaces {
        if space.status == "deleted" {
            continue;
        }
        if effective_space_role_with(store, principal, &space, &memberships)
            .await?
            .is_some()
        {
            ids.push(space.space_id);
        }
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}
