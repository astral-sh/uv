#include <Python.h>

static PyObject *answer(PyObject *self, PyObject *args) {
    return PyLong_FromLong(42);
}

static PyMethodDef methods[] = {
    {"answer", answer, METH_NOARGS, NULL},
    {NULL, NULL, 0, NULL},
};

static struct PyModuleDef module = {
    PyModuleDef_HEAD_INIT,
    "uv_test_native_extension",
    NULL,
    -1,
    methods,
    NULL,
    NULL,
    NULL,
    NULL,
};

PyMODINIT_FUNC PyInit_uv_test_native_extension(void) {
    return PyModule_Create(&module);
}
