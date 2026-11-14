package generated.routes;

import org.apache.camel.builder.RouteBuilder;
import org.apache.camel.LoggingLevel;
import jakarta.enterprise.context.ApplicationScoped;

@ApplicationScoped
public class EnrichPayloadRoute extends RouteBuilder {

    @Override
    public void configure() throws Exception {
        from("direct:enrichPayload")
            .routeId("enrichPayload")
            .setProperty("enriched", constant("true"))
            .log(LoggingLevel.DEBUG, "Payload enriched")
            ;
    }
}
