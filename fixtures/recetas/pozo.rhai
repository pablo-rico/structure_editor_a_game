// Pozo de piedra con tejadillo: receta generativa de un modelo (docs/recetas.md).
// 24×30×24 vóxeles: casilla y media de ancho. El frente es z = 0.
new_model("Objects/Voxel/pozo", 24, 30, 24);

// Brocal: anillo de piedra con vetas más oscuras y un remate algo más ancho.
shaded("8a8478");
tube(12, 12, 9, 6, 0, 8);
shaded("6e6a60");
speckle(0, 0, 0, 24, 8, 24, 25, 3, "8a8478");
shaded("a09a8c");
tube(12, 12, 10, 6, 8, 10);

// Agua en el fondo.
color("2a5a7a");
cylinder(12, 12, 6, 0, 3);

// Postes (uno y su reflejo) y travesaño.
shaded("6e4a2a");
mirror(true, false);
box(2, 10, 11, 4, 22, 13);
mirror(false, false);
cylinder_x(20.5, 12, 1, 4, 20);

// Tejadillo a dos aguas con la cumbrera a lo largo de x.
shaded("9a4e28");
roof(0, 22, 4, 24, 20, "x");

// Cubo colgando de su cuerda.
shaded("54361e");
box(11, 12, 11, 13, 15, 13);
color("d9d0b8");
line(12, 15, 12, 12, 19, 12);

print("pozo: " + count() + " vóxeles");
