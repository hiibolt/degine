//! Who can touch a workspace.
//!
//! A library read takes [`Scope`]. A library write takes [`Editor`].
//! Renaming, deleting, and membership take [`Creator`].
//! Database methods ask for one of these, so a write cannot be called
//! with a read scope.

use std::ops::Deref;

#[derive(Clone)]
pub struct Scope {
    workspace_id: String,
    user_id: String,
    email: String,
    username: String,
    editor: bool,
    creator: bool,
}

impl Scope {
    pub(crate) fn new(
        workspace_id: String,
        user_id: String,
        email: String,
        username: String,
        editor: bool,
        creator: bool,
    ) -> Self {
        Self {
            workspace_id,
            user_id,
            email,
            username,
            editor,
            creator,
        }
    }

    pub fn ws(&self) -> &str {
        &self.workspace_id
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn editor(&self) -> bool {
        self.editor || self.creator
    }

    pub fn creator(&self) -> bool {
        self.creator
    }
}

/// Proof that this request may change the library.
#[derive(Clone)]
pub struct Editor(Scope);

impl Editor {
    pub(crate) fn new(scope: Scope) -> Self {
        Self(scope)
    }
}

impl Deref for Editor {
    type Target = Scope;

    fn deref(&self) -> &Scope {
        &self.0
    }
}

/// Proof that this request is the person who created the workspace.
#[derive(Clone)]
pub struct Creator(Editor);

impl Creator {
    pub(crate) fn new(editor: Editor) -> Self {
        Self(editor)
    }
}

impl Deref for Creator {
    type Target = Editor;

    fn deref(&self) -> &Editor {
        &self.0
    }
}
