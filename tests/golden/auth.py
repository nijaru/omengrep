"""Authentication module with JWT and session handling."""

import hashlib
import secrets
from datetime import datetime, timedelta
from typing import Optional


class AuthenticationError(Exception):
    """Raised when authentication fails."""

    pass


class SessionExpiredError(AuthenticationError):
    """Raised when session has expired."""

    pass


def hash_password(password: str, salt: Optional[bytes] = None) -> tuple[str, bytes]:
    """Hash password with PBKDF2-SHA256."""
    if salt is None:
        salt = secrets.token_bytes(32)
    key = hashlib.pbkdf2_hmac("sha256", password.encode(), salt, 100000)
    return key.hex(), salt


def verify_password(password: str, stored_hash: str, salt: bytes) -> bool:
    """Verify password against stored hash."""
    computed_hash, _ = hash_password(password, salt)
    return secrets.compare_digest(computed_hash, stored_hash)


def create_session_token(user_id: str, expires_hours: int = 24) -> dict:
    """Create a new session token for user."""
    return {
        "token": secrets.token_urlsafe(32),
        "user_id": user_id,
        "created_at": datetime.utcnow().isoformat(),
        "expires_at": (datetime.utcnow() + timedelta(hours=expires_hours)).isoformat(),
    }


def validate_session(session: dict) -> bool:
    """Check if session is valid and not expired."""
    if not session or "expires_at" not in session:
        return False
    expires = datetime.fromisoformat(session["expires_at"])
    return datetime.utcnow() < expires


class UserManager:
    """Manages user authentication and sessions."""

    def __init__(self, db_connection):
        self.db = db_connection
        self._sessions: dict[str, dict] = {}

    def register_user(self, username: str, email: str, password: str) -> str:
        """Register new user with hashed password."""
        password_hash, salt = hash_password(password)
        user_id = secrets.token_hex(16)
        self.db.insert(
            "users",
            {
                "id": user_id,
                "username": username,
                "email": email,
                "password_hash": password_hash,
                "salt": salt.hex(),
            },
        )
        return user_id

    def login(self, username: str, password: str) -> dict:
        """Authenticate user and create session."""
        user = self.db.find_one("users", {"username": username})
        if not user:
            raise AuthenticationError("Invalid username or password")

        salt = bytes.fromhex(user["salt"])
        if not verify_password(password, user["password_hash"], salt):
            raise AuthenticationError("Invalid username or password")

        session = create_session_token(user["id"])
        self._sessions[session["token"]] = session
        return session

    def logout(self, token: str) -> bool:
        """Invalidate session token."""
        if token in self._sessions:
            del self._sessions[token]
            return True
        return False

    def get_current_user(self, token: str) -> Optional[dict]:
        """Get user from session token."""
        session = self._sessions.get(token)
        if not session or not validate_session(session):
            raise SessionExpiredError("Session expired or invalid")
        return self.db.find_one("users", {"id": session["user_id"]})
