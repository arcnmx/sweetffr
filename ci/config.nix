{ config, pkgs, env, lib, ... }: with pkgs; with lib; let
  pkgs = inputs.nixpkgs.legacyPackages.${builtins.currentSystem};
  sweetffr-flake = import ../. { pkgs = null; };
  lib'sweetffr = sweetffr-flake.lib;
  inherit (sweetffr-flake) inputs checks legacyPackages;
  v0' = builtins.match ''^(v)?[0-9].*$'';
  v0 = v: v != null && v0' v != null;
in {
  config = {
    name = "sweetffr";
    ci.version = "v0.7";
    ci.gh-actions = {
      enable = true;
      emit = true;
      checkoutOptions = {
        submodules = false;
      };
    };
    cache.cachix = {
      ci.signingKey = "";
      arc.enable = true;
    };
    channels = {
      nixpkgs = mkIf (env.platform != "impure") "24.05";
    };
    environment = {
      test = {
        inherit (pkgs) pkg-config;
        inherit (pkgs.stdenv) cc;
      };
    };
    tasks = {
      build.inputs = [
        checks.sweetffr
      ];
      fmt.inputs = [
        checks.rustfmt
      ];
      readme.inputs = [
        checks.readme-github
      ];
    };
    jobs = {
      nightly = { config, ... }: {
        ci.gh-actions.name = "cargo doc+fmt";
        ci.gh-actions = {
          checkoutOptions = {
            fetch-depth = 0;
          };
        };
        tasks = mkForce {
          pages = {
            cache.wrap = true;
            inputs = [
              checks.pages
            ];
          };
          publish-docs.inputs = let
            srcBranch = findFirst (v: v != null) null [ env.git-tag env.git-branch ];
          in ci.command {
            name = "publish-docs";
            displayName = "publish docs";
            impure = true;
            skip = if env.platform != "gh-actions" || env.gh-event-name or null != "push" then env.gh-event-name or "github"
              else if env.git-tag != null && ! v0 env.git-tag then "unversioned tag"
              else if env.git-branch != null && ! (elem env.git-branch lib'sweetffr.branches || v0 env.git-branch) then "feature branch"
              else if srcBranch == null then "unknown branch"
              else false;
            gitCommit = env.git-commit;
            docsBranch = "gh-pages";
            inherit srcBranch;
            releaseTag = if env.git-branch == "main" || v0 env.git-branch then lib'sweetffr.releaseTag
              else if v0 env.git-tag then env.git-tag
              else "";
            pagesDep = config.tasks.pages.drv;
            pages = legacyPackages.sweetffr-pages;
            environment = [ "CARGO_TARGET_DIR" ];
            command = ''
              git fetch origin
              if [[ -e $docsBranch ]]; then
                git worktree remove -f $docsBranch || true
                rm -rf ./$docsBranch || true
              fi
              git worktree add --detach $docsBranch && cd $docsBranch
              git branch -D pages || true
              git checkout --orphan pages && git rm -rf .
              git reset --hard origin/$docsBranch -- || true
              rm -rf "./$srcBranch"
              mkdir -p "./$srcBranch"
              cp -Lr $pages/root/* "./$srcBranch/"
              git add "$srcBranch"

              if [[ -n $releaseTag ]] && [[ $srcBranch != $releaseTag ]]; then
                ln -sfn "$srcBranch" "$releaseTag"
                git add "$releaseTag"
              fi

              if [[ -n $(git status --porcelain) ]]; then
                export GIT_{COMMITTER,AUTHOR}_EMAIL=ghost@konpaku.2hu
                export GIT_{COMMITTER,AUTHOR}_NAME=ghost
                git commit -m "$srcBranch: $gitCommit"
                git push origin HEAD:$docsBranch
              fi
            '';
          };
        };
      };
    };
  };

  options = {
    enableNightly = mkEnableOption "unstable rust";
  };
}
