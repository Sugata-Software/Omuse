#define _GNU_SOURCE
#include "LinuxFileOperations.h"
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>

int compositor_publish_file(const char *staged, const char *destination) {
    // Unlike unlink + rename, exchange never exposes a missing destination and
    // cannot destroy the last save if publication fails. It handles nonempty
    // project directories as well as regular files and destination symlinks.
    if (renameat2(AT_FDCWD, staged, AT_FDCWD, destination, RENAME_EXCHANGE) == 0)
        return 0;
    int error = errno;
    if (error != ENOENT)
        return error;

    // A new document has no destination to exchange. Do not overwrite anything
    // created concurrently after the failed exchange. Unsupported filesystems
    // fail safely; there is deliberately no delete-then-move fallback.
    if (renameat2(AT_FDCWD, staged, AT_FDCWD, destination, RENAME_NOREPLACE) == 0)
        return 0;
    return errno;
}
