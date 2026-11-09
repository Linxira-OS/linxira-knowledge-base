use crate::error::StoreError;
use crate::util::normalize_rfc3339;
use chrono::Utc;
use serde::{Deserialize, Serialize};

pub const SOLO_TENANT: &str = "t_local";
pub const SOLO_SPACE: &str = "s_default";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    Human,
    Agent,
    Service,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenScope {
    Read,
    Write,
    Admin,
}

impl TokenScope {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "read" => Ok(TokenScope::Read),
            "write" => Ok(TokenScope::Write),
            "admin" => Ok(TokenScope::Admin),
            other => Err(format!("unknown scope {other:?}; expected read, write or admin")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TokenScope::Read => "read",
            TokenScope::Write => "write",
            TokenScope::Admin => "admin",
        }
    }

    pub fn allows_read(self) -> bool {
        matches!(self, TokenScope::Read | TokenScope::Write | TokenScope::Admin)
    }

    pub fn allows_write(self) -> bool {
        matches!(self, TokenScope::Write | TokenScope::Admin)
    }

    pub fn allows_admin(self) -> bool {
        matches!(self, TokenScope::Admin)
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UserRole {
    User,
    Admin,
    Owner,
}

impl UserRole {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "user" => Ok(UserRole::User),
            "admin" => Ok(UserRole::Admin),
            "owner" => Ok(UserRole::Owner),
            other => Err(format!("unknown tenant role {other:?}; expected owner, admin or user")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            UserRole::User => "user",
            UserRole::Admin => "admin",
            UserRole::Owner => "owner",
        }
    }

    pub fn is_admin(self) -> bool {
        matches!(self, UserRole::Owner | UserRole::Admin)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpaceRole {
    Viewer,
    Editor,
    SpaceAdmin,
    SpaceOwner,
}

impl SpaceRole {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "viewer" => Ok(SpaceRole::Viewer),
            "editor" => Ok(SpaceRole::Editor),
            "space_admin" | "admin" => Ok(SpaceRole::SpaceAdmin),
            "space_owner" | "owner" => Ok(SpaceRole::SpaceOwner),
            other => Err(format!(
                "unknown space role {other:?}; expected space_owner, space_admin, editor or viewer"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SpaceRole::Viewer => "viewer",
            SpaceRole::Editor => "editor",
            SpaceRole::SpaceAdmin => "space_admin",
            SpaceRole::SpaceOwner => "space_owner",
        }
    }

    pub fn can_read(self) -> bool {
        self >= SpaceRole::Viewer
    }

    pub fn can_write(self) -> bool {
        self >= SpaceRole::Editor
    }

    pub fn can_manage_members(self) -> bool {
        self >= SpaceRole::SpaceAdmin
    }

    pub fn can_manage_space(self) -> bool {
        self >= SpaceRole::SpaceAdmin
    }

    pub fn can_own(self) -> bool {
        self >= SpaceRole::SpaceOwner
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupRole {
    GroupMember,
    GroupAdmin,
}

impl GroupRole {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "group_member" | "member" => Ok(GroupRole::GroupMember),
            "group_admin" | "admin" => Ok(GroupRole::GroupAdmin),
            other => Err(format!(
                "unknown group role {other:?}; expected group_admin or group_member"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            GroupRole::GroupMember => "group_member",
            GroupRole::GroupAdmin => "group_admin",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalType {
    User,
    Group,
    Agent,
}

impl PrincipalType {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "user" => Ok(PrincipalType::User),
            "group" => Ok(PrincipalType::Group),
            "agent" => Ok(PrincipalType::Agent),
            other => Err(format!("unknown principal type {other:?}; expected user, group or agent")),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PrincipalType::User => "user",
            PrincipalType::Group => "group",
            PrincipalType::Agent => "agent",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal {
    pub principal_id: String,
    pub display_name: String,
    pub tenant_id: String,
    pub allowed_space_ids: Vec<String>,
    pub groups: Vec<String>,
    /// Solo's explicit allow-all strategy. Never inferred from missing ACL.
    pub allow_all: bool,
    pub kind: PrincipalKind,
    pub scope: TokenScope,
    pub tenant_role: UserRole,
    pub token_id: Option<String>,
}

impl Principal {
    pub fn solo() -> Self {
        Self {
            principal_id: "local".to_string(),
            display_name: "Local user".to_string(),
            tenant_id: SOLO_TENANT.to_string(),
            allowed_space_ids: vec![SOLO_SPACE.to_string()],
            groups: Vec::new(),
            allow_all: true,
            kind: PrincipalKind::Human,
            scope: TokenScope::Admin,
            tenant_role: UserRole::Owner,
            token_id: None,
        }
    }

    pub fn local_user(user_id: &str, tenant_id: &str, spaces: Vec<String>, groups: Vec<String>) -> Self {
        Self::local_user_with_role(user_id, tenant_id, spaces, groups, UserRole::User)
    }

    pub fn local_user_with_role(
        user_id: &str,
        tenant_id: &str,
        spaces: Vec<String>,
        groups: Vec<String>,
        tenant_role: UserRole,
    ) -> Self {
        Self {
            principal_id: user_id.to_string(),
            display_name: user_id.to_string(),
            tenant_id: tenant_id.to_string(),
            allowed_space_ids: if spaces.is_empty() {
                vec![SOLO_SPACE.to_string()]
            } else {
                spaces
            },
            groups,
            allow_all: false,
            kind: PrincipalKind::Human,
            scope: if tenant_role.is_admin() {
                TokenScope::Admin
            } else {
                TokenScope::Write
            },
            tenant_role,
            token_id: None,
        }
    }

    pub fn with_token_id(mut self, token_id: Option<String>) -> Self {
        self.token_id = token_id;
        self
    }

    pub fn with_scope(mut self, scope: TokenScope) -> Self {
        self.scope = scope;
        self
    }

    pub fn with_tenant_role(mut self, role: UserRole) -> Self {
        self.tenant_role = role;
        self
    }

    pub fn can_write(&self) -> bool {
        self.scope.allows_write()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub principal: Principal,
    /// RFC3339 UTC timestamp. Visibility is evaluated at this instant.
    pub as_of: String,
    /// Optional recall-stage language filter (`zh` / `en` / `und`).
    pub lang: Option<String>,
}

impl Policy {
    pub fn new(principal: Principal, as_of: Option<&str>) -> Result<Self, String> {
        let as_of = match as_of {
            Some(value) => normalize_rfc3339(value)?,
            None => Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        };
        if principal.tenant_id.trim().is_empty() {
            return Err("principal has no tenant_id".to_string());
        }
        if principal.allowed_space_ids.is_empty()
            || principal.allowed_space_ids.iter().any(|s| s.trim().is_empty())
        {
            return Err("principal has no allowed spaces".to_string());
        }
        Ok(Self { principal, as_of, lang: None })
    }

    pub fn with_lang(mut self, lang: Option<String>) -> Self {
        self.lang = lang
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty());
        self
    }

    pub fn solo() -> Self {
        Self::new(Principal::solo(), None).expect("solo principal is valid")
    }

    pub fn visibility(&self, alias: &str, dialect: SqlDialect) -> Result<Visibility, StoreError> {
        self.visibility_from(alias, dialect, 1)
    }

    /// Build the one predicate that every recall query must use.
    ///
    /// `start` allows callers that already bound a query string (for example
    /// PostgreSQL's `$1` websearch predicate) to continue placeholder numbering.
    pub fn visibility_from(
        &self,
        alias: &str,
        dialect: SqlDialect,
        start: usize,
    ) -> Result<Visibility, StoreError> {
        let tenant = self.principal.tenant_id.trim();
        if tenant.is_empty() {
            return Err(StoreError::Invalid("tenant_id is required".into()));
        }
        if self.principal.allowed_space_ids.is_empty() {
            return Err(StoreError::Invalid("at least one space is required".into()));
        }

        let mut binds: Vec<String> = Vec::new();
        let mut next = start;

        let tenant_ph = dialect.placeholder(next);
        next += 1;
        binds.push(tenant.to_string());

        let mut space_placeholders = Vec::new();
        for space in &self.principal.allowed_space_ids {
            let trimmed = space.trim();
            if trimmed.is_empty() {
                return Err(StoreError::Invalid("empty space_id in policy".into()));
            }
            space_placeholders.push(dialect.placeholder(next));
            next += 1;
            binds.push(trimmed.to_string());
        }

        let from_ph = dialect.placeholder(next);
        next += 1;
        binds.push(self.as_of.clone());

        let to_ph = dialect.placeholder(next);
        next += 1;
        binds.push(self.as_of.clone());

        let mut acl_patterns: Vec<String> = Vec::new();
        if self.principal.allow_all {
            acl_patterns.push("|*|".to_string());
        } else {
            acl_patterns.push("|*|".to_string());
            acl_patterns.push(format!("|u:{}|", self.principal.principal_id));
            for group in &self.principal.groups {
                acl_patterns.push(format!("|g:{group}|"));
            }
        }
        acl_patterns.sort();
        acl_patterns.dedup();

        let mut acl_clauses = Vec::new();
        for pattern in &acl_patterns {
            let ph = dialect.placeholder(next);
            next += 1;
            binds.push(format!("%{}%", escape_like(pattern)));
            acl_clauses.push(format!(
                "{alias}.acl_principals LIKE {ph} ESCAPE '\\'"
            ));
        }

        let lang_clause = if let Some(lang) = &self.lang {
            let ph = dialect.placeholder(next);
            binds.push(lang.clone());
            format!(" AND {alias}.lang = {ph}")
        } else {
            String::new()
        };

        let sql = format!(
            "{alias}.tenant_id = {tenant_ph} \
             AND {alias}.space_id IN ({spaces}) \
             AND {alias}.valid_from <= {from_ph} \
             AND ({alias}.valid_to IS NULL OR {alias}.valid_to > {to_ph}) \
             AND ({acl}){lang_clause}",
            spaces = space_placeholders.join(", "),
            acl = acl_clauses.join(" OR "),
        );

        Ok(Visibility { sql, binds })
    }

    /// Secondary, in-process ACL/temporal verification after recall. This is
    /// not the primary filter; it exists to catch stale index metadata.
    pub fn can_read(&self, fields: &GovernanceFields) -> bool {
        if fields.tenant_id != self.principal.tenant_id {
            return false;
        }
        if !self
            .principal
            .allowed_space_ids
            .iter()
            .any(|space| space == &fields.space_id)
        {
            return false;
        }
        if let Some(lang) = &self.lang {
            if lang != &fields.lang {
                return false;
            }
        }
        if fields.valid_from.as_str() > self.as_of.as_str() {
            return false;
        }
        if let Some(valid_to) = &fields.valid_to {
            if valid_to.as_str() <= self.as_of.as_str() {
                return false;
            }
        }
        match Acl::from_json(&fields.acl_json) {
            Ok(acl) => acl.allows(&self.principal),
            Err(_) => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Visibility {
    pub sql: String,
    pub binds: Vec<String>,
}

fn escape_like(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '%' => out.push_str("\\%"),
            '_' => out.push_str("\\_"),
            _ => out.push(ch),
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlDialect {
    Sqlite,
    Postgres,
}

impl SqlDialect {
    pub fn placeholder(self, index: usize) -> String {
        match self {
            SqlDialect::Sqlite => "?".to_string(),
            SqlDialect::Postgres => format!("${index}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GovernanceFields {
    pub tenant_id: String,
    pub space_id: String,
    pub acl_json: String,
    pub acl_principals: String,
    pub lang: String,
    pub valid_from: String,
    pub valid_to: Option<String>,
    pub superseded_by: Option<String>,
    pub version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AclMode {
    AllowAll,
    AllowList,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Acl {
    pub mode: AclMode,
    #[serde(default)]
    pub users: Vec<String>,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default = "default_acl_version")]
    pub version: u32,
}

fn default_acl_version() -> u32 {
    1
}

impl Default for Acl {
    fn default() -> Self {
        Self::allow_all()
    }
}

impl Acl {
    pub fn allow_all() -> Self {
        Self {
            mode: AclMode::AllowAll,
            users: Vec::new(),
            groups: Vec::new(),
            version: 1,
        }
    }

    pub fn allow_list(users: Vec<String>, groups: Vec<String>) -> Self {
        let mut users = users;
        let mut groups = groups;
        users.sort();
        users.dedup();
        groups.sort();
        groups.dedup();
        Self {
            mode: AclMode::AllowList,
            users,
            groups,
            version: 1,
        }
    }

    pub fn user(user_id: &str) -> Self {
        Self::allow_list(vec![user_id.to_string()], Vec::new())
    }

    pub fn from_json(value: &str) -> Result<Self, StoreError> {
        serde_json::from_str(value)
            .map_err(|err| StoreError::Invalid(format!("invalid ACL JSON: {err}")))
    }

    pub fn from_user_input(value: &str) -> Result<Self, String> {
        serde_json::from_str(value).map_err(|err| format!("invalid ACL JSON: {err}"))
    }

    pub fn to_json(&self) -> String {
        // Field order is fixed by the struct declaration, which keeps the
        // serialized form stable for tests and for LIKE-based visibility.
        serde_json::to_string(self).expect("ACL serialization cannot fail")
    }

    /// Compact index form used by the visibility predicate:
    ///   allow_all  -> `|*|`
    ///   allow_list -> `|u:alice|g:eng|`
    pub fn principals_key(&self) -> String {
        if self.mode == AclMode::AllowAll {
            return "|*|".to_string();
        }
        let mut parts: Vec<String> = Vec::new();
        for user in &self.users {
            parts.push(format!("u:{user}"));
        }
        for group in &self.groups {
            parts.push(format!("g:{group}"));
        }
        parts.sort();
        parts.dedup();
        format!("|{}|", parts.join("|"))
    }

    pub fn allows(&self, principal: &Principal) -> bool {
        if self.mode == AclMode::AllowAll {
            return true;
        }
        if self.users.iter().any(|u| u == &principal.principal_id) {
            return true;
        }
        principal
            .groups
            .iter()
            .any(|g| self.groups.iter().any(|allowed| allowed == g))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit_for(acl: &Acl) -> GovernanceFields {
        GovernanceFields {
            tenant_id: "t1".into(),
            space_id: "s1".into(),
            acl_json: acl.to_json(),
            acl_principals: acl.principals_key(),
            lang: "zh".into(),
            valid_from: "2026-01-01T00:00:00Z".into(),
            valid_to: None,
            superseded_by: None,
            version: 1,
        }
    }

    #[test]
    fn solo_visibility_is_still_a_sql_predicate() {
        let policy = Policy::solo();
        let visibility = policy.visibility("c", SqlDialect::Sqlite).unwrap();
        assert!(visibility.sql.contains("c.tenant_id = ?"));
        assert!(visibility.sql.contains("c.acl_principals LIKE ?"));
        assert!(visibility.binds.contains(&"|*|".to_string()) == false);
        assert!(visibility
            .binds
            .iter()
            .any(|b| b.contains("|*|")));
    }

    #[test]
    fn allow_list_checks_user_and_groups() {
        let acl = Acl::allow_list(vec!["alice".into()], vec!["eng".into()]);
        let mut alice = Principal::local_user("alice", "t1", vec!["s1".into()], vec![]);
        assert!(acl.allows(&alice));
        let mut bob = Principal::local_user("bob", "t1", vec!["s1".into()], vec!["eng".into()]);
        assert!(acl.allows(&bob));
        bob.groups.clear();
        assert!(!acl.allows(&bob));
        assert!(!acl.allows(&Principal::local_user("carol", "t1", vec!["s1".into()], vec![])));
        alice.groups.clear();
        let policy = Policy::new(alice, None).unwrap();
        assert!(policy.can_read(&hit_for(&acl)));
    }

    #[test]
    fn unknown_acl_fails_closed() {
        let policy = Policy::new(
            Principal::local_user("alice", "t1", vec!["s1".into()], vec![]),
            None,
        )
        .unwrap();
        let mut fields = hit_for(&Acl::allow_all());
        fields.acl_json = "not json".into();
        assert!(!policy.can_read(&fields));
    }

    #[test]
    fn temporal_window_is_enforced() {
        let policy = Policy::new(
            Principal::solo(),
            Some("2026-01-01T00:00:00Z"),
        )
        .unwrap();
        let mut fields = hit_for(&Acl::allow_all());
        fields.tenant_id = SOLO_TENANT.into();
        fields.space_id = SOLO_SPACE.into();
        fields.valid_from = "2026-02-01T00:00:00Z".into();
        assert!(!policy.can_read(&fields));
        fields.valid_from = "2025-01-01T00:00:00Z".into();
        fields.valid_to = Some("2025-12-01T00:00:00Z".into());
        assert!(!policy.can_read(&fields));
        fields.valid_to = Some("2027-01-01T00:00:00Z".into());
        fields.superseded_by = Some("doc_new".into());
        assert!(policy.can_read(&fields));
    }

    #[test]
    fn empty_principal_is_rejected() {
        let mut principal = Principal::solo();
        principal.allowed_space_ids.clear();
        assert!(Policy::new(principal, None).is_err());
    }
}
