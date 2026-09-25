pub type Uid = u32;
pub type Gid = u32;

pub const ROOT_UID: Uid = 0;
pub const ROOT_GID: Gid = 0;

pub const BOI_UID: Uid = 1000;
pub const BOI_GID: Gid = 1000;

#[derive(Clone, Copy)]
pub struct User {
    pub uid: Uid,
    pub gid: Gid,
    pub name: &'static str,
    pub home: &'static str,
    pub shell: &'static str,
    pub groups: &'static [Gid],
}

#[derive(Clone, Copy)]
pub struct Group {
    pub gid: Gid,
    pub name: &'static str,
}

#[derive(Clone, Copy)]
pub struct Credentials {
    pub ruid: Uid,
    pub rgid: Gid,

    pub euid: Uid,
    pub egid: Gid,

    pub suid: Uid,
    pub sgid: Gid,

    pub fsuid: Uid,
    pub fsgid: Gid,
}

impl Credentials {
    pub const fn root() -> Self {
        Self {
            ruid: ROOT_UID,
            rgid: ROOT_GID,

            euid: ROOT_UID,
            egid: ROOT_GID,

            suid: ROOT_UID,
            sgid: ROOT_GID,

            fsuid: ROOT_UID,
            fsgid: ROOT_GID,
        }
    }

    pub const fn user(uid: Uid, gid: Gid) -> Self {
        Self {
            ruid: uid,
            rgid: gid,

            euid: uid,
            egid: gid,

            suid: uid,
            sgid: gid,

            fsuid: uid,
            fsgid: gid,
        }
    }

    pub const fn is_root(self) -> bool {
        self.euid == ROOT_UID
    }
}

static ROOT_GROUPS: [Gid; 1] = [ROOT_GID];

static BOI_GROUPS: [Gid; 1] = [BOI_GID];

pub const ROOT: User = User {
    uid: ROOT_UID,
    gid: ROOT_GID,
    name: "root",
    home: "/root",
    shell: "/bin/bash",
    groups: &ROOT_GROUPS,
};

pub const BOI: User = User {
    uid: BOI_UID,
    gid: BOI_GID,
    name: "boi",
    home: "/home/boi",
    shell: "/bin/bash",
    groups: &BOI_GROUPS,
};

pub const ROOT_GROUP: Group = Group {
    gid: ROOT_GID,
    name: "root",
};

pub const BOI_GROUP: Group = Group {
    gid: BOI_GID,
    name: "boi",
};

pub const USERS: &[User] = &[ROOT, BOI];

pub const GROUPS: &[Group] = &[ROOT_GROUP, BOI_GROUP];

#[derive(Clone, Copy)]
pub struct Session {
    user: User,
    credentials: Credentials,
    cwd: &'static str,
    umask: u16,
}

impl Session {
    pub const fn initial() -> Self {
        Self {
            user: BOI,
            credentials: Credentials::user(BOI_UID, BOI_GID),
            cwd: BOI.home,
            umask: 0o022,
        }
    }

    pub const fn root() -> Self {
        Self {
            user: ROOT,
            credentials: Credentials::root(),
            cwd: ROOT.home,
            umask: 0o022,
        }
    }

    pub const fn user(self) -> User {
        self.user
    }

    pub const fn credentials(self) -> Credentials {
        self.credentials
    }

    pub const fn cwd(self) -> &'static str {
        self.cwd
    }

    pub const fn umask(self) -> u16 {
        self.umask
    }

    pub fn set_cwd(&mut self, cwd: &'static str) {
        self.cwd = cwd;
    }
}

pub fn user_by_name(name: &[u8]) -> Option<User> {
    for user in USERS {
        if name == user.name.as_bytes() {
            return Some(*user);
        }
    }

    None
}

pub fn user_by_uid(uid: Uid) -> Option<User> {
    for user in USERS {
        if user.uid == uid {
            return Some(*user);
        }
    }

    None
}

pub fn group_by_name(name: &[u8]) -> Option<Group> {
    for group in GROUPS {
        if name == group.name.as_bytes() {
            return Some(*group);
        }
    }

    None
}

pub fn group_by_gid(gid: Gid) -> Option<Group> {
    for group in GROUPS {
        if group.gid == gid {
            return Some(*group);
        }
    }

    None
}
