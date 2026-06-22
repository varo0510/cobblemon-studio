// Oculta la ventana de consola de Windows SIEMPRE (también en debug), para que la app arranque limpia.
#![windows_subsystem = "windows"]

fn main() {
    cobblemon_studio_lib::run()
}
