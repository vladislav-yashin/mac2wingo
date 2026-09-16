/*
 * SPDX-License-Identifier: MIT
 *
 * hiveset - set a single value in an offline Windows registry hive.
 *
 * This file is part of Mac2Win Go and is licensed under the MIT License (see
 * LICENSE). The resulting binary is dynamically linked against libhivex, which
 * is licensed under the LGPL-2.1-only; see THIRD_PARTY_NOTICES.md.
 *
 * The bundled `hivexsh` only exposes `setval`, which replaces *all* values at
 * the current node (it calls hivex_node_set_values). Editing shared keys such
 * as \ControlSet001\Control with it would wipe unrelated values, so the Windows
 * To Go offline configuration uses this helper instead: it calls
 * hivex_node_set_value, which replaces exactly one (key, value) pair and leaves
 * every other value untouched.
 *
 * Usage: hiveset <hive> <path> <name> dword:<value>
 *   <path>  Windows path from the hive root, e.g. \ControlSet001\Control.
 *   <value> accepts decimal, octal (0...) and hex (0x...) like hivexsh.
 */

#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "hivex.h"

static void
usage (const char *prog)
{
  fprintf (stderr, "usage: %s <hive> <path> <name> dword:<value>\n", prog);
  exit (EXIT_FAILURE);
}

/* Resolve a Windows path such as "\ControlSet001\Control\Power" from the root
 * node. Returns 0 if any component is missing. */
static hive_node_h
find_node (hive_h *h, const char *path)
{
  hive_node_h node = hivex_root (h);
  const char *p = path;

  while (*p) {
    while (*p == '\\')
      p++;
    if (*p == '\0')
      break;

    const char *end = strchr (p, '\\');
    size_t len = end ? (size_t) (end - p) : strlen (p);

    char *element = malloc (len + 1);
    if (element == NULL) {
      perror ("malloc");
      exit (EXIT_FAILURE);
    }
    memcpy (element, p, len);
    element[len] = '\0';

    hive_node_h child = hivex_node_get_child (h, node, element);
    if (child == 0) {
      fprintf (stderr, "hiveset: %s: subkey not found in %s\n", element, path);
      free (element);
      return 0;
    }
    free (element);
    node = child;

    if (end == NULL)
      break;
    p = end;
  }

  return node;
}

int
main (int argc, char *argv[])
{
  if (argc != 5)
    usage (argv[0]);

  const char *hivefile = argv[1];
  const char *path = argv[2];
  const char *name = argv[3];
  const char *rawvalue = argv[4];

  if (strncmp (rawvalue, "dword:", 6) != 0) {
    fprintf (stderr, "hiveset: only dword:<value> is supported, got: %s\n",
             rawvalue);
    return EXIT_FAILURE;
  }

  char *endptr = NULL;
  errno = 0;
  unsigned long value = strtoul (rawvalue + 6, &endptr, 0);
  if (errno != 0 || endptr == rawvalue + 6 || *endptr != '\0'
      || value > UINT32_MAX) {
    fprintf (stderr, "hiveset: invalid dword value: %s\n", rawvalue + 6);
    return EXIT_FAILURE;
  }

  hive_h *h = hivex_open (hivefile, HIVEX_OPEN_WRITE);
  if (h == NULL) {
    fprintf (stderr, "hiveset: failed to open %s: %s\n", hivefile,
             strerror (errno));
    return EXIT_FAILURE;
  }

  hive_node_h node = find_node (h, path);
  if (node == 0) {
    hivex_close (h);
    return EXIT_FAILURE;
  }

  /* Windows DWORDs are stored little endian, independent of the host. */
  unsigned char data[4] = {
    (unsigned char) (value & 0xff),
    (unsigned char) ((value >> 8) & 0xff),
    (unsigned char) ((value >> 16) & 0xff),
    (unsigned char) ((value >> 24) & 0xff),
  };
  hive_set_value val = {
    .key = (char *) name,
    .t = hive_t_dword,
    .len = sizeof data,
    .value = (char *) data,
  };

  int ret = EXIT_SUCCESS;
  if (hivex_node_set_value (h, node, &val, 0) == -1) {
    fprintf (stderr, "hiveset: setting %s: %s\n", name, strerror (errno));
    ret = EXIT_FAILURE;
  }
  else if (hivex_commit (h, NULL, 0) == -1) {
    fprintf (stderr, "hiveset: commit: %s\n", strerror (errno));
    ret = EXIT_FAILURE;
  }

  hivex_close (h);
  return ret;
}
