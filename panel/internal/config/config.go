package config

import (
	"os"
	"path/filepath"

	"github.com/spf13/viper"
)

// Config holds runtime settings (env > defaults).
type Config struct {
	Port       int
	DataDir    string
	AnsibleDir string
	StaticDir  string
	TmplDir    string
}

// Load reads env vars (PORT, PANEL_DATA, PANEL_ANSIBLE) with sane defaults.
func Load() Config {
	v := viper.New()
	v.SetDefault("port", 8080)
	v.SetDefault("data_dir", "./data")
	v.AutomaticEnv()
	v.BindEnv("port", "PORT")
	v.BindEnv("data_dir", "PANEL_DATA")
	v.BindEnv("ansible_dir", "PANEL_ANSIBLE")

	cwd, _ := os.Getwd()
	dataDir := v.GetString("data_dir")
	if !filepath.IsAbs(dataDir) {
		dataDir = filepath.Join(cwd, dataDir)
	}
	return Config{
		Port:       v.GetInt("port"),
		DataDir:    dataDir,
		AnsibleDir: resolveAnsible(v.GetString("ansible_dir"), cwd),
		StaticDir:  filepath.Join(cwd, "web", "static"),
		TmplDir:    filepath.Join(cwd, "web", "templates"),
	}
}

func resolveAnsible(env, cwd string) string {
	if env != "" {
		return env
	}
	for _, c := range []string{
		filepath.Join(cwd, "..", "ansible"),
		"/usr/share/proxpilot/ansible",
	} {
		if st, err := os.Stat(filepath.Join(c, "playbook-common.yml")); err == nil && !st.IsDir() {
			return c
		}
	}
	return "/usr/share/proxpilot/ansible"
}
