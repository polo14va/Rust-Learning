// ============================================================================
// TYPE-STATE PATTERN: User Registration Builder
// ============================================================================
//
// PROBLEMA QUE RESUELVE:
// En código tradicional, podríamos crear un usuario sin username o password:
//
//   let user = User { username: None, password: None }; // ❌ Compila pero es inválido
//
// Con Type-State, el compilador OBLIGA a configurar todo antes de .build():
//
//   UserRegistration::new()
//       .username("admin")      // Cambia el tipo: NoUsername -> NoPassword
//       .password("secret")     // Cambia el tipo: NoPassword -> Ready
//       .build()                // Solo Ready tiene .build()
//
// Si olvidas .username() o .password(), el código NO COMPILA.
//
// ============================================================================

use std::marker::PhantomData;

// ----------------------------------------------------------------------------
// ESTADOS (Tipos vacíos que solo existen en compile-time)
// ----------------------------------------------------------------------------
// Estos tipos NO ocupan memoria (son marcadores para el compilador)

/// Estado inicial: No se ha configurado username
pub struct NoUsername;

/// Estado intermedio: Username configurado, falta password
pub struct NoPassword;

/// Estado final: Todo configurado, listo para crear usuario
pub struct Ready;

// ----------------------------------------------------------------------------
// BUILDER CON TYPE-STATE
// ----------------------------------------------------------------------------

pub struct UserRegistration<State> {
    username: Option<String>,
    password: Option<String>,
    email: Option<String>,
    
    // PhantomData<State> es un marcador de tipo que:
    // 1. NO ocupa memoria (zero-cost abstraction)
    // 2. Le dice al compilador qué "estado" tiene este builder
    _state: PhantomData<State>,
}

// ----------------------------------------------------------------------------
// IMPLEMENTACIÓN: Estado NoUsername
// ----------------------------------------------------------------------------
// Solo este estado tiene el constructor .new()

impl UserRegistration<NoUsername> {
    /// Crea un nuevo builder en estado inicial
    pub fn new() -> Self {
        UserRegistration {
            username: None,
            password: None,
            email: None,
            _state: PhantomData,
        }
    }

    /// Configura el username y CAMBIA EL TIPO a NoPassword
    /// 
    /// NOTA: Consume `self` y devuelve un UserRegistration<NoPassword>
    /// Esto hace imposible usar el builder anterior (fue "movido")
    pub fn username(self, username: impl Into<String>) -> UserRegistration<NoPassword> {
        UserRegistration {
            username: Some(username.into()),
            password: self.password,
            email: self.email,
            _state: PhantomData, // Cambiamos el tipo del estado
        }
    }
}

// ----------------------------------------------------------------------------
// IMPLEMENTACIÓN: Estado NoPassword
// ----------------------------------------------------------------------------
// Solo este estado tiene .password() y .email()

impl UserRegistration<NoPassword> {
    /// Configura el password y CAMBIA EL TIPO a Ready
    pub fn password(self, password: impl Into<String>) -> UserRegistration<Ready> {
        UserRegistration {
            username: self.username,
            password: Some(password.into()),
            email: self.email,
            _state: PhantomData,
        }
    }

    /// Configura email (opcional) sin cambiar el estado
    pub fn email(self, email: impl Into<String>) -> Self {
        UserRegistration {
            username: self.username,
            password: self.password,
            email: Some(email.into()),
            _state: PhantomData,
        }
    }
}

// ----------------------------------------------------------------------------
// IMPLEMENTACIÓN: Estado Ready
// ----------------------------------------------------------------------------
// SOLO este estado tiene .build() - garantiza que username y password existen

impl UserRegistration<Ready> {
    /// Email opcional (se puede configurar antes de .password())
    pub fn email(self, email: impl Into<String>) -> Self {
        UserRegistration {
            username: self.username,
            password: self.password,
            email: Some(email.into()),
            _state: PhantomData,
        }
    }

    /// Construye los datos finales
    /// 
    /// GARANTÍA DEL COMPILADOR: username y password SIEMPRE existen aquí
    /// No necesitamos .unwrap() peligroso ni validaciones runtime
    pub fn build(self) -> (String, String, Option<String>) {
        (
            self.username.unwrap(), // Safe: garantizado por el tipo Ready
            self.password.unwrap(), // Safe: garantizado por el tipo Ready
            self.email,
        )
    }
}

// ============================================================================
// EJEMPLO DE USO (en handlers.rs)
// ============================================================================
//
// ✅ CORRECTO (compila):
//
//   let (username, password, email) = UserRegistration::new()
//       .username("admin")
//       .password("secret123")
//       .email("admin@example.com")  // Opcional
//       .build();
//
// ❌ INCORRECTO (NO compila):
//
//   let data = UserRegistration::new()
//       .username("admin")
//       .build();  // ERROR: método `build` no existe en UserRegistration<NoPassword>
//
// ❌ INCORRECTO (NO compila):
//
//   let data = UserRegistration::new()
//       .password("secret")  // ERROR: método `password` no existe en UserRegistration<NoUsername>
//       .build();
//
// ============================================================================
