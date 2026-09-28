#ifndef COMPOSITOR_LINUX_FILE_OPERATIONS_H
#define COMPOSITOR_LINUX_FILE_OPERATIONS_H

// Atomically publish a staged file or directory. Returns zero or an errno value.
// On failure, neither path is removed. On replacement, the old destination is
// moved to staged so the caller can remove it after the commit point.
int compositor_publish_file(const char *staged, const char *destination);

#endif
