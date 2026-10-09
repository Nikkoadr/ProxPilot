package auth

import (
	"net/http"
	"time"

	"github.com/gin-gonic/gin"
	"golang.org/x/crypto/bcrypt"
	"proxpilot/internal/store"
)

const (
	CookieName = "proxpilot_session"
	SessionTTL = 7 * 24 * time.Hour
)

// EnsureSeed creates default admin/admin123 on first boot.
func EnsureSeed(st *store.Store, user, pass string) error {
	if st.UserCount() > 0 {
		return nil
	}
	if user == "" {
		user = "admin"
	}
	if pass == "" {
		pass = "admin123"
	}
	h, err := Hash(pass)
	if err != nil {
		return err
	}
	if err := st.SetPassHash(user, h); err != nil {
		return err
	}
	return st.Set("default_creds", "1")
}

func Hash(pass string) (string, error) {
	h, err := bcrypt.GenerateFromPassword([]byte(pass), bcrypt.DefaultCost)
	return string(h), err
}

func Check(hash, pass string) bool {
	return bcrypt.CompareHashAndPassword([]byte(hash), []byte(pass)) == nil
}

// Middleware aborts with 401 for API or redirects HTML to login.
func Middleware(st *store.Store) gin.HandlerFunc {
	return func(c *gin.Context) {
		tok, err := c.Cookie(CookieName)
		if err == nil {
			if _, ok := st.CheckSession(tok); ok {
				c.Next()
				return
			}
		}
		if isPage(c.Request.URL.Path) {
			c.Redirect(http.StatusFound, "/login")
			c.Abort()
			return
		}
		c.AbortWithStatusJSON(http.StatusUnauthorized, gin.H{"error": "login required"})
	}
}

func isPage(p string) bool {
	if p == "/" {
		return true
	}
	if len(p) >= 5 && p[len(p)-5:] == ".html" {
		return true
	}
	for _, ch := range p {
		if ch == '.' {
			return false
		}
	}
	return true
}
