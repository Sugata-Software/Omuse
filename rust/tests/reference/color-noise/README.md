# Portable color-noise regression packet

This allocator-controlled C harness and review-only patch accompany the
[upstream contribution notes](../../../../docs/upstream-color-noise-reproduction.md).
Run `scripts/reproduce-upstream-color-noise.sh` from the repository root.

The patch retains the original upstream path so it can be reviewed against
that repository. The script applies it only to a temporary copy of the
independent reference kernel. It never modifies Omuse or the retained
reference files.
