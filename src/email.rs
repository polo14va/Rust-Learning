use crate::error::AppError;

/// Simula el envío de un email. Por ahora solo imprime en consola/tracing.
/// Más adelante se sustituirá por un proveedor real.
pub async fn send_email(to: &str, subject: &str, body: &str) -> Result<(), AppError> {
    tracing::info!(
        target: "email_simulator",
        "Simulando envío de email -> TO: {to}, SUBJECT: {subject}, BODY: {body}"
    );
    println!("(Email simulado) To: {to}\nSubject: {subject}\n\n{body}");
    Ok(())
}
