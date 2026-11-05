package main

import (
	"context"

	"dagger/quintodrome/internal/dagger"
)

type Quintodrome struct{}

// Ci runs the full Linux CI: lint, build, and test for both Go and JS.
func (m *Quintodrome) Ci(ctx context.Context,
	// +defaultPath="."
	src *dagger.Directory,
) error {
	if err := m.LintGo(ctx, src); err != nil {
		return err
	}
	if err := m.FmtGo(ctx, src); err != nil {
		return err
	}
	if err := m.BuildGo(ctx, src); err != nil {
		return err
	}
	if err := m.TestGo(ctx, src); err != nil {
		return err
	}
	if err := m.LintJS(ctx, src); err != nil {
		return err
	}
	if err := m.BuildJS(ctx, src); err != nil {
		return err
	}
	if err := m.TestJS(ctx, src); err != nil {
		return err
	}
	return nil
}

// LintGo runs golangci-lint with the repo's .golangci.yml.
func (m *Quintodrome) LintGo(ctx context.Context, src *dagger.Directory) error {
	_, err := goContainer().
		WithDirectory("/repo", src).
		WithWorkdir("/repo").
		// The version upstream pins (Makefile's GOLANGCI_LINT_VERSION), so it
		// moves with upstream merges; one built with an older Go refuses a
		// go.mod that needs a newer one. The release tarball directly, not the
		// install script (whose hardcoded SHA256 for v2.12.0 didn't match the
		// re-uploaded tarball), for this machine's architecture.
		WithExec([]string{"sh", "-c", `
set -e
v=$(grep '^GOLANGCI_LINT_VERSION' Makefile | cut -d ' ' -f 3)
v=${v#v}
curl -sSfL "https://github.com/golangci/golangci-lint/releases/download/v$v/golangci-lint-$v-linux-$(go env GOARCH).tar.gz" | tar -xz --strip-components=1 -C /usr/local/bin
`}).
		WithExec([]string{"golangci-lint", "run", "--timeout", "2m"}).
		Sync(ctx)
	return err
}

// FmtGo checks that goimports and go mod tidy leave the tree clean.
func (m *Quintodrome) FmtGo(ctx context.Context, src *dagger.Directory) error {
	_, err := goContainer().
		WithDirectory("/repo", src).
		WithWorkdir("/repo").
		WithExec([]string{"sh", "-c", `
go run golang.org/x/tools/cmd/goimports@latest -w $(find . -name '*.go' | grep -v '_gen.go$' | grep -v '.pb.go$')
go mod tidy
test -z "$(git status --porcelain)"
`}).
		Sync(ctx)
	return err
}

// BuildGo compiles the Navidrome server and all packages.
func (m *Quintodrome) BuildGo(ctx context.Context, src *dagger.Directory) error {
	_, err := goContainer().
		WithDirectory("/repo", src).
		WithWorkdir("/repo").
		WithExec([]string{"go", "build", "-tags", "netgo,sqlite_fts5", "./..."}).
		Sync(ctx)
	return err
}

// TestGo runs the Go test suite.
func (m *Quintodrome) TestGo(ctx context.Context, src *dagger.Directory) error {
	_, err := goContainer().
		WithDirectory("/repo", src).
		WithWorkdir("/repo").
		WithExec([]string{"go", "test", "-tags", "netgo,sqlite_fts5", "./..."}).
		Sync(ctx)
	return err
}

// LintJS runs prettier and eslint in the web UI.
func (m *Quintodrome) LintJS(ctx context.Context, src *dagger.Directory) error {
	_, err := jsContainer().
		WithDirectory("/repo", src).
		WithWorkdir("/repo/ui").
		WithExec([]string{"npm", "ci", "--ignore-scripts"}).
		WithExec([]string{"npm", "run", "check-formatting"}).
		WithExec([]string{"npm", "run", "lint"}).
		Sync(ctx)
	return err
}

// BuildJS builds the Navidrome web UI.
func (m *Quintodrome) BuildJS(ctx context.Context, src *dagger.Directory) error {
	_, err := jsContainer().
		WithDirectory("/repo", src).
		WithWorkdir("/repo/ui").
		WithExec([]string{"npm", "ci", "--ignore-scripts"}).
		WithExec([]string{"npm", "run", "build"}).
		Sync(ctx)
	return err
}

// TestJS runs the JS test suite.
func (m *Quintodrome) TestJS(ctx context.Context, src *dagger.Directory) error {
	_, err := jsContainer().
		WithDirectory("/repo", src).
		WithWorkdir("/repo/ui").
		WithExec([]string{"npm", "ci", "--ignore-scripts"}).
		WithExec([]string{"npm", "test"}).
		Sync(ctx)
	return err
}

// The containers below keep their package and build caches in cache
// volumes, which outlive a run in the (shared, long-running) dagger engine:
// without them every step downloaded every Go module, or ran npm ci, from
// scratch. Go's caches and npm's are safe to share between steps and between
// runs at the same time.

func goContainer() *dagger.Container {
	return dag.Container().
		From("golang:1.27").
		WithExec([]string{"apt-get", "update"}).
		WithExec([]string{"apt-get", "install", "-y", "zip"}).
		WithMountedCache("/go/pkg/mod", dag.CacheVolume("quintodrome-go-mod")).
		WithMountedCache("/root/.cache/go-build", dag.CacheVolume("quintodrome-go-build")).
		WithMountedCache("/root/.cache/golangci-lint", dag.CacheVolume("quintodrome-golangci-lint"))
}

func jsContainer() *dagger.Container {
	return dag.Container().
		From("node:24").
		WithMountedCache("/root/.npm", dag.CacheVolume("quintodrome-npm"))
}

// BuildDesktopWindows builds the Windows desktop app (.exe/.msi) inside a
// Windows container.
//
// Experimental: this requires the Dagger engine to run on a Windows host with
// BuildKit's Windows container support enabled (the engine uses the containerd
// worker with `platforms = ["windows/amd64"]`). Unverified end-to-end — the
// Rust MSVC toolchain and Tauri's NSIS/WiX bundling may not behave in a
// Windows Server Core container the way they do on a full Windows runner.
func (m *Quintodrome) BuildDesktopWindows(ctx context.Context, src *dagger.Directory) *dagger.Directory {
	setup := `Set-ExecutionPolicy Bypass -Scope Process -Force
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor 3072
iex ((New-Object Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
choco install -y nodejs-lts golang rustup.install visualstudio2022buildtools visualstudio2022-workload-vctools --no-progress`

	return dag.Container().
		From("mcr.microsoft.com/windows/servercore:ltsc2022").
		WithDirectory("C:/src", src).
		WithWorkdir("C:/src").
		WithExec([]string{"powershell", "-NoProfile", "-Command", setup}).
		WithExec([]string{"cmd", "/c", "npm --version && go version && rustc --version"}).
		Directory("C:/src/desktop/src-tauri/target/release/bundle")
}
