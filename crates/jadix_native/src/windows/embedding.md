Create an owned child root inside an existing native parent window.

# Safety

The caller must own or have permission to integrate with the parent HWND and
keep its window procedure and UI dispatcher valid while that window exists.
Create, poll and dispose the child on the parent's UI thread. This function
checks the live HWND and current thread before creating the child; ownership of
the parent remains with its caller. Destroying the parent revokes the child root.
