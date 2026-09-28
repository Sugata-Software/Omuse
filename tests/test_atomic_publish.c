#define _GNU_SOURCE
#include "LinuxFileOperations.h"
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

// Inject publication failures without filling a disk or depending on root or
// mount privileges. All non-injected calls use the real filesystem primitive.
int __real_renameat2(int, const char *, int, const char *, unsigned int);
static int failure;
static int concurrent_creation;

static void write_file(const char *path, const char *text) {
    FILE *file = fopen(path, "w");
    assert(file);
    assert(fputs(text, file) >= 0);
    assert(fclose(file) == 0);
}

static void expect_file(const char *path, const char *text) {
    char contents[32] = {0};
    FILE *file = fopen(path, "r");
    assert(file);
    assert(fread(contents, 1, sizeof(contents) - 1, file) == strlen(text));
    assert(fclose(file) == 0);
    assert(strcmp(contents, text) == 0);
}

int __wrap_renameat2(int oldfd, const char *oldpath, int newfd, const char *newpath, unsigned int flags) {
    if (failure) { errno = failure; return -1; }
    if (concurrent_creation && flags == RENAME_NOREPLACE) {
        write_file(newpath, "concurrent");
        concurrent_creation = 0;
    }
    return __real_renameat2(oldfd, oldpath, newfd, newpath, flags);
}

int main(void) {
    char root[] = "/tmp/compositor-publish-XXXXXX";
    assert(mkdtemp(root));
    char staged[256], destination[256];
    snprintf(staged, sizeof(staged), "%s/staged", root);
    snprintf(destination, sizeof(destination), "%s/destination", root);
    write_file(staged, "new");
    write_file(destination, "old");
    const int errors[] = {ENOSPC, EACCES, EOPNOTSUPP};
    for (size_t i = 0; i < sizeof(errors) / sizeof(errors[0]); ++i) {
        failure = errors[i];
        assert(compositor_publish_file(staged, destination) == failure);
        expect_file(staged, "new");
        expect_file(destination, "old");
    }
    failure = 0;
    assert(compositor_publish_file(staged, destination) == 0);
    expect_file(staged, "old");
    expect_file(destination, "new");
    assert(unlink(destination) == 0);
    concurrent_creation = 1;
    assert(compositor_publish_file(staged, destination) == EEXIST);
    expect_file(staged, "old");
    expect_file(destination, "concurrent");
    assert(unlink(destination) == 0);
    assert(compositor_publish_file(staged, destination) == 0);
    expect_file(destination, "old");
    assert(access(staged, F_OK) == -1 && errno == ENOENT);
    assert(unlink(destination) == 0);
    assert(rmdir(root) == 0);
    puts("Atomic publish: failed exchange preserves both versions; create refuses a concurrent writer");
}
